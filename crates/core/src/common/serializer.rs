use std::error::Error;

use super::{
    query_result::ExpressionResult,
    types::{Dump, DumpFormat},
};
use csv::WriterBuilder;

pub(crate) fn dump_results(result: &ExpressionResult, dump: &Dump) -> Result<(), Box<dyn Error>> {
    let content = serialize_results(result, dump)?;
    std::fs::write(dump.path(), content)?;
    Ok(())
}

pub fn serialize_results(result: &ExpressionResult, dump: &Dump) -> Result<String, Box<dyn Error>> {
    match dump.format {
        DumpFormat::Json => serialize_json(result),
        DumpFormat::Csv => serialize_csv(result),
        DumpFormat::Parquet => serialize_parquet(result),
    }
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

fn serialize_parquet(_results: &ExpressionResult) -> Result<String, Box<dyn Error>> {
    unimplemented!()
}

#[cfg(test)]
mod test {
    use std::str::FromStr;

    use alloy::primitives::U256;

    use crate::common::{
        query_result::{AccountQueryRes, ExpressionResult},
        serializer::serialize_results,
        types::{Dump, DumpFormat},
    };

    #[test]
    fn test_serialize_json() {
        let res = AccountQueryRes {
            address: None,
            balance: Some(U256::from_str("100").unwrap()),
            nonce: Some(0),
            code: None,
        };

        let result = ExpressionResult::Account(vec![res]);
        let dump = Dump::new("test.json".to_string(), DumpFormat::Json);

        let content = serialize_results(&result, &dump).unwrap();

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
        let dump_file = Dump::new("test.csv".to_string(), DumpFormat::Csv);

        let content = serialize_results(&result, &dump_file).unwrap();

        assert_eq!(content, "nonce,balance\n0,100\n");
    }
}
