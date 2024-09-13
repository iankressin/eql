use std::error::Error;
use std::sync::Arc;

use super::{
    query_result::ExpressionResult,
    types::{Dump, DumpFormat},
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
            let content = serialize_csv(result)?;
            std::fs::write(dump.path(), content)?;
        }
        DumpFormat::Parquet => {
            let content = serialize_parquet(result)?;
            std::fs::write(dump.path(), content)?;
        }
    }
    Ok(())
}

fn serialize_json(result: &ExpressionResult) -> Result<String, Box<dyn Error>> {
    Ok(serde_json::to_string_pretty(result)?)
}

fn serialize_csv(result: &ExpressionResult) -> Result<String, Box<dyn Error>> {
    let mut writer = WriterBuilder::new().has_headers(true).from_writer(vec![]);

    match result {
        ExpressionResult::Account(accounts) => writer.serialize(accounts)?,
        ExpressionResult::Block(blocks) => writer.serialize(blocks)?,
        ExpressionResult::Transaction(transactions) => writer.serialize(transactions)?,
        ExpressionResult::Log(logs) => writer.serialize(logs)?,
    }

    let content = writer.into_inner()?;
    Ok(String::from_utf8(content)?)
}

fn serialize_parquet(result: &ExpressionResult) -> Result<Vec<u8>, Box<dyn Error>> {
    let (schema, data) = match result {
        ExpressionResult::Account(accounts) => create_schema_and_data(accounts)?,
        ExpressionResult::Block(blocks) => create_schema_and_data(blocks)?,
        ExpressionResult::Transaction(transactions) => create_schema_and_data(transactions)?,
        ExpressionResult::Log(logs) => create_schema_and_data(logs)?,
    };

    let batch = RecordBatch::try_new(Arc::new(schema), data)?;

    let mut buf = Vec::new();
    let mut writer = ArrowWriter::try_new(&mut buf, batch.schema(), None)?;

    writer.write(&batch)?;
    writer.close()?;

    Ok(buf)
}

fn create_schema_and_data<T: Serialize>(
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
    use super::{serialize_csv, serialize_json, serialize_parquet};
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
        };
        let result = ExpressionResult::Account(vec![res]);
        let content = serialize_json(&result).unwrap();

        assert_eq!(content, "{\n  \"account\": [\n    {\n      \"nonce\": 0,\n      \"balance\": \"100\"\n    }\n  ]\n}");
    }

    #[test]
    fn test_serialize_csv() {
        let res = AccountQueryRes {
            address: None,
            balance: Some(U256::from_str("100").unwrap()),
            nonce: Some(0),
            code: None,
        };
        let result = ExpressionResult::Account(vec![res]);
        let content = serialize_csv(&result).unwrap();

        assert_eq!(content, "nonce,balance\n0,100\n");
    }

    #[test]
    fn test_serialize_parquet() {
        let res = AccountQueryRes {
            address: None,
            balance: Some(U256::from_str("100").unwrap()),
            nonce: Some(0),
            code: None,
        };
        let result = ExpressionResult::Account(vec![res]);
        let content = serialize_parquet(&result).unwrap();

        // Since Parquet is a binary format, we can't easily assert its content.
        // Instead, we'll just check that we get a non-empty result.
        assert!(!content.is_empty());
    }
}
