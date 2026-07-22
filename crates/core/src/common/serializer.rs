use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;

use super::{
    dump::{Dump, DumpFormat},
    query_result::ExpressionResult,
};
use arrow::array::{ArrayRef, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
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
    let (schema, data) = match result {
        ExpressionResult::Account(accounts) => create_parquet_schema_and_data(accounts)?,
        ExpressionResult::Block(blocks) => create_parquet_schema_and_data(blocks)?,
        ExpressionResult::Transaction(transactions) => {
            create_parquet_schema_and_data(transactions)?
        }
        ExpressionResult::Log(logs) => create_parquet_schema_and_data(logs)?,
    };

    let batch = RecordBatch::try_new(Arc::new(schema), data)?;

    let mut buf = Vec::new();
    let mut writer = ArrowWriter::try_new(&mut buf, batch.schema(), None)?;

    writer.write(&batch)?;
    writer.close()?;

    Ok(buf)
}

fn create_parquet_schema_and_data<T: Serialize>(
    items: &[T],
) -> Result<(Schema, Vec<ArrayRef>), Box<dyn Error>> {
    let mut fields = Vec::new();
    let mut data = Vec::new();

    if let Some(first_item) = items.first() {
        let value = serde_json::to_value(first_item)?;
        if let serde_json::Value::Object(map) = value {
            for (key, _) in map {
                let field = Field::new(&key, DataType::Utf8, true);
                fields.push(field);

                let column_data: Vec<Option<String>> = items
                    .iter()
                    .map(|item| {
                        let item_value = serde_json::to_value(item).unwrap();
                        if let serde_json::Value::Object(item_map) = item_value {
                            item_map.get(&key).and_then(|v| Some(v.to_string()))
                        } else {
                            None
                        }
                    })
                    .collect();

                data.push(Arc::new(StringArray::from(column_data)) as ArrayRef);
            }
        }
    }

    let schema = Schema::new(fields);
    Ok((schema, data))
}

#[cfg(test)]
mod test {
    use super::{apply_aliases, serialize_csv, serialize_json, serialize_parquet};
    use crate::common::query_result::{AccountQueryRes, ExpressionResult};
    use alloy::primitives::U256;
    use std::str::FromStr;

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
