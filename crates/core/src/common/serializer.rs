use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;

use super::{
    dump::{Dump, DumpFormat},
    query_result::{
        AccountQueryRes, BlockQueryRes, ExpressionResult, LogQueryRes, TransactionQueryRes,
    },
};
use alloy::primitives::U256;
use arrow::array::{ArrayRef, BooleanArray, Decimal128Array, StringArray, UInt64Array, UInt8Array};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::{RecordBatch, RecordBatchOptions};
use parquet::arrow::ArrowWriter;
use serde::Serialize;

use csv::WriterBuilder;

pub(crate) fn dump_results(result: &ExpressionResult, dump: &Dump) -> Result<(), Box<dyn Error>> {
    match dump.format {
        DumpFormat::Json => {
            let content = serialize_json(result)?;
            std::fs::write(dump.path(), content)?;
        }
        DumpFormat::Csv => {
            let content = match result {
                ExpressionResult::Account(accounts) => serialize_csv(accounts)?,
                ExpressionResult::Block(blocks) => serialize_csv(blocks)?,
                ExpressionResult::Transaction(txs) => serialize_csv(txs)?,
                ExpressionResult::Log(logs) => serialize_csv(logs)?,
            };

            std::fs::write(dump.path(), content)?;
        }
        DumpFormat::Parquet => {
            let content = serialize_parquet(result)?;
            std::fs::write(dump.path(), content)?;
        }
    }
    Ok(())
}

fn serialize_json<T: Serialize>(result: &T) -> Result<String, Box<dyn Error>> {
    Ok(serde_json::to_string_pretty(result)?)
}

/// Renames object keys found in `aliases` (mapping original column name ->
/// alias) throughout a `serde_json::Value`, recursing into arrays and
/// objects. Keys not present in `aliases` are left untouched. If two source
/// keys map to the same alias (or an alias collides with an existing
/// unaliased key), the later write wins per `serde_json::Map`'s insertion
/// semantics; the `aliases` `HashMap` is only ever queried by `.get()`, so
/// its iteration order plays no part. What decides "later" is the row's own
/// key order — this crate does not enable serde_json's `preserve_order`
/// feature anywhere in the workspace, so `serde_json::Map` is `BTreeMap`
/// -backed and iterates keys alphabetically. In practice that means: when
/// two original column names alias to the same target, whichever name
/// sorts later alphabetically wins that slot, and the other's value is
/// lost. This is an implementation detail, not a documented guarantee — it
/// would silently change if `preserve_order` were ever turned on.
pub(crate) fn apply_aliases(value: &mut serde_json::Value, aliases: &HashMap<String, String>) {
    match value {
        serde_json::Value::Array(items) => {
            for item in items {
                apply_aliases(item, aliases);
            }
        }
        serde_json::Value::Object(map) => {
            let keys: Vec<String> = map.keys().cloned().collect();
            for key in keys {
                if let Some(alias) = aliases.get(&key) {
                    if let Some(v) = map.remove(&key) {
                        map.insert(alias.clone(), v);
                    }
                }
            }
        }
        _ => {}
    }
}

/// Same as `dump_results`'s `DumpFormat::Json` branch, except object keys in
/// the rows are renamed per `aliases` before the file is written. The
/// top-level shape — `{"<entity>": [...]}` — matches `dump_results` exactly;
/// only the inner row keys differ. Callers are responsible for rejecting
/// `aliases` paired with a non-JSON `DumpFormat` before reaching here (see
/// `ExecutionEngine::run_get_expr`) — CSV/Parquet aliasing is out of scope
/// for this v1.
pub(crate) fn dump_results_with_aliases(
    result: &ExpressionResult,
    dump: &Dump,
    aliases: &HashMap<String, String>,
) -> anyhow::Result<()> {
    let mut value = serde_json::to_value(result)?;
    if let serde_json::Value::Object(map) = &mut value {
        for rows in map.values_mut() {
            apply_aliases(rows, aliases);
        }
    }
    std::fs::write(dump.path(), serde_json::to_string_pretty(&value)?)?;
    Ok(())
}

fn serialize_csv<T: Serialize>(results: &Vec<T>) -> Result<String, Box<dyn Error>> {
    let mut writer = WriterBuilder::new().has_headers(true).from_writer(vec![]);

    for result in results {
        writer.serialize(result)?
    }

    Ok(String::from_utf8(writer.into_inner()?)?)
}

fn serialize_parquet(result: &ExpressionResult) -> Result<Vec<u8>, Box<dyn Error>> {
    let (columns, num_rows) = match result {
        ExpressionResult::Account(rows) => (account_columns(rows)?, rows.len()),
        ExpressionResult::Block(rows) => (block_columns(rows)?, rows.len()),
        ExpressionResult::Transaction(rows) => (transaction_columns(rows)?, rows.len()),
        ExpressionResult::Log(rows) => (log_columns(rows)?, rows.len()),
    };

    let (fields, arrays): (Vec<Field>, Vec<ArrayRef>) = columns.into_iter().unzip();
    let schema = Arc::new(Schema::new(fields));

    // With no columns (an empty result, or a query whose selected fields are
    // all null) Arrow can't infer the row count from the arrays, so pass it.
    let batch = if arrays.is_empty() {
        RecordBatch::try_new_with_options(
            schema.clone(),
            arrays,
            &RecordBatchOptions::new().with_row_count(Some(num_rows)),
        )?
    } else {
        RecordBatch::try_new(schema.clone(), arrays)?
    };

    let mut buf = Vec::new();
    let mut writer = ArrowWriter::try_new(&mut buf, schema, None)?;

    writer.write(&batch)?;
    writer.close()?;

    Ok(buf)
}

// --- Typed Parquet columns ---------------------------------------------------
//
// Each entity's row struct (see `query_result.rs`) maps onto Arrow columns with
// a proper type per field, read straight off the struct rather than through
// `serde_json`. A column is emitted only when at least one row sets the field:
// unselected fields don't produce all-null columns, and the schema no longer
// depends on which fields happen to be present in the first row.

type Column = (Field, ArrayRef);

fn all_none<T>(vals: &[Option<T>]) -> bool {
    vals.iter().all(Option::is_none)
}

fn col<R, T>(rows: &[R], f: impl Fn(&R) -> Option<T>) -> Vec<Option<T>> {
    rows.iter().map(f).collect()
}

fn push(cols: &mut Vec<Column>, column: Option<Column>) {
    if let Some(column) = column {
        cols.push(column);
    }
}

fn u64_col(name: &str, vals: Vec<Option<u64>>) -> Option<Column> {
    if all_none(&vals) {
        return None;
    }
    Some((
        Field::new(name, DataType::UInt64, true),
        Arc::new(UInt64Array::from(vals)) as ArrayRef,
    ))
}

fn u8_col(name: &str, vals: Vec<Option<u8>>) -> Option<Column> {
    if all_none(&vals) {
        return None;
    }
    Some((
        Field::new(name, DataType::UInt8, true),
        Arc::new(UInt8Array::from(vals)) as ArrayRef,
    ))
}

fn bool_col(name: &str, vals: Vec<Option<bool>>) -> Option<Column> {
    if all_none(&vals) {
        return None;
    }
    Some((
        Field::new(name, DataType::Boolean, true),
        Arc::new(BooleanArray::from(vals)) as ArrayRef,
    ))
}

fn str_col(name: &str, vals: Vec<Option<String>>) -> Option<Column> {
    if all_none(&vals) {
        return None;
    }
    Some((
        Field::new(name, DataType::Utf8, true),
        Arc::new(StringArray::from(vals)) as ArrayRef,
    ))
}

/// Quantity `U256` fields (balances, values, sizes, difficulty) as
/// `Decimal128(38, 0)`. 38 decimal digits hold every real chain quantity (the
/// largest conceivable native balance is far below 10^38 wei), and DuckDB and
/// Polars — whose `DECIMAL` maxes out at precision 38 — read it back exactly,
/// whereas a wider `Decimal256` would be downcast to a lossy `double`. A column
/// carrying a value that doesn't fit — only a synthetic one, e.g. a `U256::MAX`
/// test-net balance — falls back to lossless decimal strings rather than being
/// truncated. Full-range signature fields (`r`/`s`) are always decimal strings.
fn u256_col(name: &str, vals: Vec<Option<U256>>) -> Result<Option<Column>, Box<dyn Error>> {
    if all_none(&vals) {
        return Ok(None);
    }
    // 10^38 is Decimal128(38, 0)'s exclusive upper bound.
    let limit = U256::from(10).pow(U256::from(38));
    if vals.iter().flatten().all(|v| *v < limit) {
        let values: Vec<Option<i128>> = vals
            .into_iter()
            .map(|v| v.map(u256_to_i128).transpose())
            .collect::<Result<_, _>>()?;
        let array = Decimal128Array::from(values).with_precision_and_scale(38, 0)?;
        Ok(Some((
            Field::new(name, DataType::Decimal128(38, 0), true),
            Arc::new(array) as ArrayRef,
        )))
    } else {
        Ok(str_col(name, decimal_strings(vals)))
    }
}

/// A `U256` known to be below 10^38 as an `i128` for `Decimal128`. The
/// conversions can't fail given that bound, but are checked rather than
/// wrapping so a caller that ignored the bound gets an error, not a bad value.
fn u256_to_i128(value: U256) -> Result<i128, Box<dyn Error>> {
    Ok(i128::try_from(u128::try_from(value)?)?)
}

/// `u128` gas fields as `Decimal128(38, 0)` — 38 digits covers every realistic
/// gas value. A value at/above 10^38 (never a real gas price) can't fit and
/// makes the column fall back to lossless decimal strings, same as `u256_col`.
fn u128_col(name: &str, vals: Vec<Option<u128>>) -> Result<Option<Column>, Box<dyn Error>> {
    if all_none(&vals) {
        return Ok(None);
    }
    let limit = 10u128.pow(38);
    if vals.iter().flatten().all(|v| *v < limit) {
        let values: Vec<Option<i128>> = vals.into_iter().map(|v| v.map(|u| u as i128)).collect();
        let array = Decimal128Array::from(values).with_precision_and_scale(38, 0)?;
        Ok(Some((
            Field::new(name, DataType::Decimal128(38, 0), true),
            Arc::new(array) as ArrayRef,
        )))
    } else {
        Ok(str_col(
            name,
            vals.into_iter().map(|v| v.map(|u| u.to_string())).collect(),
        ))
    }
}

/// Render each present `U256` as its decimal string (for the fallback column).
fn decimal_strings(vals: Vec<Option<U256>>) -> Vec<Option<String>> {
    vals.into_iter().map(|v| v.map(|u| u.to_string())).collect()
}

fn account_columns(rows: &[AccountQueryRes]) -> Result<Vec<Column>, Box<dyn Error>> {
    let mut cols = Vec::new();
    push(
        &mut cols,
        str_col(
            "chain",
            col(rows, |r| r.chain.as_ref().map(|c| c.to_string())),
        ),
    );
    push(&mut cols, u64_col("nonce", col(rows, |r| r.nonce)));
    push(&mut cols, u256_col("balance", col(rows, |r| r.balance))?);
    push(
        &mut cols,
        str_col(
            "address",
            col(rows, |r| r.address.as_ref().map(|a| format!("{a:?}"))),
        ),
    );
    push(
        &mut cols,
        str_col(
            "code",
            col(rows, |r| r.code.as_ref().map(|c| format!("{c:?}"))),
        ),
    );
    Ok(cols)
}

fn block_columns(rows: &[BlockQueryRes]) -> Result<Vec<Column>, Box<dyn Error>> {
    let mut cols = Vec::new();
    push(
        &mut cols,
        str_col(
            "chain",
            col(rows, |r| r.chain.as_ref().map(|c| c.to_string())),
        ),
    );
    push(&mut cols, u64_col("number", col(rows, |r| r.number)));
    push(&mut cols, u64_col("timestamp", col(rows, |r| r.timestamp)));
    push(
        &mut cols,
        str_col(
            "hash",
            col(rows, |r| r.hash.as_ref().map(|h| format!("{h:?}"))),
        ),
    );
    push(&mut cols, u256_col("size", col(rows, |r| r.size))?);
    push(
        &mut cols,
        str_col(
            "parent_hash",
            col(rows, |r| r.parent_hash.as_ref().map(|h| format!("{h:?}"))),
        ),
    );
    push(
        &mut cols,
        str_col(
            "state_root",
            col(rows, |r| r.state_root.as_ref().map(|h| format!("{h:?}"))),
        ),
    );
    push(
        &mut cols,
        str_col(
            "transactions_root",
            col(rows, |r| {
                r.transactions_root.as_ref().map(|h| format!("{h:?}"))
            }),
        ),
    );
    push(
        &mut cols,
        str_col(
            "receipts_root",
            col(rows, |r| r.receipts_root.as_ref().map(|h| format!("{h:?}"))),
        ),
    );
    push(
        &mut cols,
        str_col(
            "logs_bloom",
            col(rows, |r| r.logs_bloom.as_ref().map(|b| format!("{b:?}"))),
        ),
    );
    push(
        &mut cols,
        str_col(
            "extra_data",
            col(rows, |r| r.extra_data.as_ref().map(|b| format!("{b:?}"))),
        ),
    );
    push(
        &mut cols,
        str_col(
            "mix_hash",
            col(rows, |r| r.mix_hash.as_ref().map(|h| format!("{h:?}"))),
        ),
    );
    push(
        &mut cols,
        u256_col("total_difficulty", col(rows, |r| r.total_difficulty))?,
    );
    push(
        &mut cols,
        u64_col("base_fee_per_gas", col(rows, |r| r.base_fee_per_gas)),
    );
    push(
        &mut cols,
        str_col(
            "withdrawals_root",
            col(rows, |r| {
                r.withdrawals_root.as_ref().map(|h| format!("{h:?}"))
            }),
        ),
    );
    push(
        &mut cols,
        u64_col("blob_gas_used", col(rows, |r| r.blob_gas_used)),
    );
    push(
        &mut cols,
        u64_col("excess_blob_gas", col(rows, |r| r.excess_blob_gas)),
    );
    push(
        &mut cols,
        str_col(
            "parent_beacon_block_root",
            col(rows, |r| {
                r.parent_beacon_block_root
                    .as_ref()
                    .map(|h| format!("{h:?}"))
            }),
        ),
    );
    Ok(cols)
}

fn transaction_columns(rows: &[TransactionQueryRes]) -> Result<Vec<Column>, Box<dyn Error>> {
    let mut cols = Vec::new();
    push(
        &mut cols,
        str_col(
            "chain",
            col(rows, |r| r.chain.as_ref().map(|c| c.to_string())),
        ),
    );
    push(&mut cols, u8_col("type", col(rows, |r| r.r#type)));
    push(
        &mut cols,
        str_col(
            "hash",
            col(rows, |r| r.hash.as_ref().map(|h| format!("{h:?}"))),
        ),
    );
    push(
        &mut cols,
        u64_col("block_number", col(rows, |r| r.block_number)),
    );
    push(
        &mut cols,
        str_col(
            "from_address",
            col(rows, |r| r.from_address.as_ref().map(|a| format!("{a:?}"))),
        ),
    );
    push(
        &mut cols,
        str_col(
            "to_address",
            col(rows, |r| r.to_address.as_ref().map(|a| format!("{a:?}"))),
        ),
    );
    push(
        &mut cols,
        str_col(
            "data",
            col(rows, |r| r.data.as_ref().map(|d| format!("{d:?}"))),
        ),
    );
    push(&mut cols, u256_col("value", col(rows, |r| r.value))?);
    push(
        &mut cols,
        u128_col("gas_price", col(rows, |r| r.gas_price))?,
    );
    push(&mut cols, u64_col("gas_limit", col(rows, |r| r.gas_limit)));
    push(
        &mut cols,
        u128_col("effective_gas_price", col(rows, |r| r.effective_gas_price))?,
    );
    push(&mut cols, bool_col("status", col(rows, |r| r.status)));
    push(&mut cols, u64_col("chain_id", col(rows, |r| r.chain_id)));
    push(&mut cols, bool_col("v", col(rows, |r| r.v)));
    push(
        &mut cols,
        str_col("r", col(rows, |r| r.r.as_ref().map(|x| x.to_string()))),
    );
    push(
        &mut cols,
        str_col("s", col(rows, |r| r.s.as_ref().map(|x| x.to_string()))),
    );
    push(
        &mut cols,
        u128_col(
            "max_fee_per_blob_gas",
            col(rows, |r| r.max_fee_per_blob_gas),
        )?,
    );
    push(
        &mut cols,
        u128_col("max_fee_per_gas", col(rows, |r| r.max_fee_per_gas))?,
    );
    push(
        &mut cols,
        u128_col(
            "max_priority_fee_per_gas",
            col(rows, |r| r.max_priority_fee_per_gas),
        )?,
    );
    push(&mut cols, bool_col("y_parity", col(rows, |r| r.y_parity)));
    push(
        &mut cols,
        str_col(
            "authorization_list",
            col(rows, |r| {
                r.authorization_list
                    .as_ref()
                    .map(|a| serde_json::to_string(a).unwrap_or_default())
            }),
        ),
    );
    Ok(cols)
}

fn log_columns(rows: &[LogQueryRes]) -> Result<Vec<Column>, Box<dyn Error>> {
    let mut cols = Vec::new();
    push(
        &mut cols,
        str_col(
            "chain",
            col(rows, |r| r.chain.as_ref().map(|c| c.to_string())),
        ),
    );
    push(
        &mut cols,
        str_col(
            "address",
            col(rows, |r| r.address.as_ref().map(|a| format!("{a:?}"))),
        ),
    );
    push(
        &mut cols,
        str_col(
            "topic0",
            col(rows, |r| r.topic0.as_ref().map(|t| format!("{t:?}"))),
        ),
    );
    push(
        &mut cols,
        str_col(
            "topic1",
            col(rows, |r| r.topic1.as_ref().map(|t| format!("{t:?}"))),
        ),
    );
    push(
        &mut cols,
        str_col(
            "topic2",
            col(rows, |r| r.topic2.as_ref().map(|t| format!("{t:?}"))),
        ),
    );
    push(
        &mut cols,
        str_col(
            "topic3",
            col(rows, |r| r.topic3.as_ref().map(|t| format!("{t:?}"))),
        ),
    );
    push(
        &mut cols,
        str_col(
            "data",
            col(rows, |r| r.data.as_ref().map(|d| format!("{d:?}"))),
        ),
    );
    push(
        &mut cols,
        str_col(
            "block_hash",
            col(rows, |r| r.block_hash.as_ref().map(|h| format!("{h:?}"))),
        ),
    );
    push(
        &mut cols,
        u64_col("block_number", col(rows, |r| r.block_number)),
    );
    push(
        &mut cols,
        u64_col("block_timestamp", col(rows, |r| r.block_timestamp)),
    );
    push(
        &mut cols,
        str_col(
            "transaction_hash",
            col(rows, |r| {
                r.transaction_hash.as_ref().map(|h| format!("{h:?}"))
            }),
        ),
    );
    push(
        &mut cols,
        u64_col("transaction_index", col(rows, |r| r.transaction_index)),
    );
    push(&mut cols, u64_col("log_index", col(rows, |r| r.log_index)));
    push(&mut cols, bool_col("removed", col(rows, |r| r.removed)));
    Ok(cols)
}

#[cfg(test)]
mod test {
    use super::{
        account_columns, apply_aliases, block_columns, log_columns, serialize_csv, serialize_json,
        serialize_parquet, transaction_columns, Column,
    };
    use crate::common::query_result::{
        AccountQueryRes, BlockQueryRes, ExpressionResult, LogQueryRes, TransactionQueryRes,
    };
    use alloy::primitives::{B256, U256};
    use arrow::array::{StringArray, UInt64Array};
    use arrow::datatypes::DataType;
    use std::collections::HashMap;
    use std::str::FromStr;

    fn column_types(cols: &[Column]) -> HashMap<String, DataType> {
        cols.iter()
            .map(|(f, _)| (f.name().clone(), f.data_type().clone()))
            .collect()
    }

    #[test]
    fn test_serialize_json() {
        let res = AccountQueryRes {
            address: None,
            balance: Some(U256::from_str("100").unwrap()),
            nonce: Some(0),
            code: None,
            chain: None,
        };
        let result = ExpressionResult::Account(vec![res]);
        let content = serialize_json(&result).unwrap();

        assert_eq!(content, "{\n  \"account\": [\n    {\n      \"nonce\": 0,\n      \"balance\": \"100\"\n    }\n  ]\n}");
    }

    #[test]
    fn test_serialize_csv() {
        let res = vec![
            AccountQueryRes {
                address: None,
                balance: Some(U256::from_str("100").unwrap()),
                nonce: Some(0),
                code: None,
                chain: None,
            },
            AccountQueryRes {
                address: None,
                balance: Some(U256::from_str("200").unwrap()),
                nonce: Some(1),
                code: None,
                chain: None,
            },
        ];
        let content = serialize_csv(&res).unwrap();

        assert_eq!(content, "nonce,balance\n0,100\n1,200\n");
    }

    #[test]
    fn test_serialize_parquet() {
        let res = AccountQueryRes {
            address: None,
            balance: Some(U256::from_str("100").unwrap()),
            nonce: Some(0),
            code: None,
            chain: None,
        };
        let result = ExpressionResult::Account(vec![res]);
        let content = serialize_parquet(&result).unwrap();

        // Since Parquet is a binary format, we can't easily assert its content.
        // Instead, we'll just check that we get a non-empty result.
        assert!(!content.is_empty());
    }

    #[test]
    fn parquet_account_columns_are_typed() {
        let rows = vec![AccountQueryRes {
            nonce: Some(3),
            balance: Some(U256::from(100u64)),
            address: Some(alloy::primitives::Address::ZERO),
            ..Default::default()
        }];
        let cols = account_columns(&rows).unwrap();
        let types = column_types(&cols);

        assert_eq!(types["nonce"], DataType::UInt64);
        assert_eq!(types["balance"], DataType::Decimal128(38, 0));
        assert_eq!(types["address"], DataType::Utf8);
        // `chain`/`code` were never set, so they aren't columns.
        assert!(!types.contains_key("chain"));
        assert!(!types.contains_key("code"));
    }

    #[test]
    fn parquet_block_columns_are_typed() {
        let rows = vec![BlockQueryRes {
            number: Some(21_000_000),
            timestamp: Some(1_700_000_000),
            size: Some(U256::from(1234u64)),
            hash: Some(B256::ZERO),
            base_fee_per_gas: Some(7),
            ..Default::default()
        }];
        let cols = block_columns(&rows).unwrap();
        let types = column_types(&cols);

        assert_eq!(types["number"], DataType::UInt64);
        assert_eq!(types["timestamp"], DataType::UInt64);
        assert_eq!(types["size"], DataType::Decimal128(38, 0));
        assert_eq!(types["hash"], DataType::Utf8);
        // An unset field never becomes a column.
        assert!(!types.contains_key("parent_hash"));

        let number = cols
            .iter()
            .find(|(f, _)| f.name() == "number")
            .unwrap()
            .1
            .as_any()
            .downcast_ref::<UInt64Array>()
            .unwrap();
        assert_eq!(number.value(0), 21_000_000);
    }

    #[test]
    fn parquet_transaction_columns_are_typed() {
        let rows = vec![TransactionQueryRes {
            status: Some(true),
            gas_price: Some(1_000_000_000),
            value: Some(U256::from(42u64)),
            r: Some(U256::from(7u64)),
            authorization_list: Some(vec![]),
            ..Default::default()
        }];
        let cols = transaction_columns(&rows).unwrap();
        let types = column_types(&cols);

        assert_eq!(types["status"], DataType::Boolean);
        assert_eq!(types["gas_price"], DataType::Decimal128(38, 0));
        assert_eq!(types["value"], DataType::Decimal128(38, 0));
        // Full-range signature components stay as decimal strings.
        assert_eq!(types["r"], DataType::Utf8);
        assert_eq!(types["authorization_list"], DataType::Utf8);
    }

    #[test]
    fn parquet_log_columns_are_typed() {
        let rows = vec![LogQueryRes {
            removed: Some(false),
            topic0: Some(B256::ZERO),
            block_number: Some(123),
            log_index: Some(4),
            ..Default::default()
        }];
        let cols = log_columns(&rows).unwrap();
        let types = column_types(&cols);

        assert_eq!(types["removed"], DataType::Boolean);
        assert_eq!(types["topic0"], DataType::Utf8);
        assert_eq!(types["block_number"], DataType::UInt64);
        assert_eq!(types["log_index"], DataType::UInt64);
    }

    #[test]
    fn parquet_u256_beyond_decimal128_falls_back_to_string() {
        // A value that overflows Decimal128(38, 0) — here U256::MAX, the kind
        // of synthetic balance a dev/test net genesis sets — must not corrupt
        // or fail the dump. The whole column degrades to lossless decimal text.
        let huge = U256::MAX;
        let rows = vec![
            AccountQueryRes {
                balance: Some(U256::from(100u64)),
                ..Default::default()
            },
            AccountQueryRes {
                balance: Some(huge),
                ..Default::default()
            },
        ];
        let cols = account_columns(&rows).unwrap();
        let types = column_types(&cols);
        assert_eq!(types["balance"], DataType::Utf8);

        let balance = cols
            .iter()
            .find(|(f, _)| f.name() == "balance")
            .unwrap()
            .1
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        assert_eq!(balance.value(0), "100");
        assert_eq!(balance.value(1), huge.to_string());
    }

    #[test]
    fn parquet_u256_within_decimal128_stays_numeric() {
        // A large-but-representable balance (10^30 wei ≈ 10^12 ETH, far above
        // any real holding yet well under 10^38) stays a Decimal128 column.
        let big = U256::from(10u64).pow(U256::from(30u64));
        let rows = vec![AccountQueryRes {
            balance: Some(big),
            ..Default::default()
        }];
        let cols = account_columns(&rows).unwrap();
        assert_eq!(column_types(&cols)["balance"], DataType::Decimal128(38, 0));
    }

    #[test]
    fn aliases_rename_json_keys() {
        let mut value = serde_json::json!([{ "balance": "1", "nonce": "2" }]);
        let aliases =
            std::collections::HashMap::from([("balance".to_string(), "eth_balance".to_string())]);
        apply_aliases(&mut value, &aliases);
        assert_eq!(value[0].get("eth_balance").unwrap(), "1");
        assert!(value[0].get("balance").is_none());
        assert!(value[0].get("nonce").is_some());
    }

    #[test]
    fn aliases_that_name_no_field_are_ignored() {
        let mut value = serde_json::json!([{ "balance": "1" }]);
        let aliases = std::collections::HashMap::from([(
            "does_not_exist".to_string(),
            "whatever".to_string(),
        )]);
        apply_aliases(&mut value, &aliases);
        assert_eq!(value[0].get("balance").unwrap(), "1");
        assert!(value[0].get("whatever").is_none());
    }

    #[test]
    fn alias_colliding_with_another_column_overwrites_it() {
        // `SELECT nonce AS balance, balance FROM ...`: aliasing `nonce` to
        // `balance` collides with the unaliased `balance` column already in
        // the row. `apply_aliases` iterates the row's own keys (a
        // `serde_json::Map`, `BTreeMap`-backed here since this workspace
        // never enables serde_json's `preserve_order` feature) in
        // alphabetical order: "balance" is visited first and left alone
        // (it isn't aliased), then "nonce" is visited and inserted under
        // "balance", overwriting it. So the surviving value is always
        // nonce's ("5"), never balance's original ("100") — deterministic
        // given this map implementation, not a coin flip.
        let mut value = serde_json::json!([{ "balance": "100", "nonce": "5" }]);
        let aliases =
            std::collections::HashMap::from([("nonce".to_string(), "balance".to_string())]);
        apply_aliases(&mut value, &aliases);
        let obj = value[0].as_object().unwrap();
        assert_eq!(obj.len(), 1);
        assert_eq!(obj.get("balance").unwrap(), "5");
        assert!(obj.get("nonce").is_none());
    }

    #[test]
    fn two_fields_aliased_to_the_same_name_collide() {
        // `SELECT balance AS x, nonce AS x FROM ...`: both source columns
        // target the same alias "x". Same alphabetical row-key order as
        // above: "balance" is processed first (aliased to "x", value
        // "100"), then "nonce" is processed and also aliased to "x",
        // overwriting it with "5". The alphabetically-later source column
        // always wins the shared alias slot.
        let mut value = serde_json::json!([{ "balance": "100", "nonce": "5" }]);
        let aliases = std::collections::HashMap::from([
            ("balance".to_string(), "x".to_string()),
            ("nonce".to_string(), "x".to_string()),
        ]);
        apply_aliases(&mut value, &aliases);
        let obj = value[0].as_object().unwrap();
        assert_eq!(obj.len(), 1);
        assert_eq!(obj.get("x").unwrap(), "5");
    }
}
