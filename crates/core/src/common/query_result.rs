use alloy::primitives::{Address, Bloom, Bytes, FixedBytes, B256, U256};
use csv::Writer;
use serde::{Deserialize, Serialize, Serializer};

use super::types::{Dump, DumpFormat};

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Clone)]
pub struct QueryResult {
    pub query: String,
    pub result: ExpressionResult,
}

impl QueryResult {
    pub fn new(query: String, result: ExpressionResult) -> QueryResult {
        QueryResult { query, result }
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Clone)]
pub enum ExpressionResult {
    #[serde(rename = "account")]
    Account(Vec<AccountQueryRes>),
    #[serde(rename = "block")]
    Block(Vec<BlockQueryRes>),
    #[serde(rename = "transaction")]
    Transaction(Vec<TransactionQueryRes>),
}

// TODO: should this be replaced with Alloy's Block?
#[serde_with::skip_serializing_none]
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Clone)]
pub struct BlockQueryRes {
    pub number: Option<u64>,
    pub timestamp: Option<u64>,
    pub hash: Option<B256>,
    #[serde(serialize_with = "serialize_option_u256")]
    pub size: Option<U256>,
    pub parent_hash: Option<B256>,
    pub state_root: Option<B256>,
    pub transactions_root: Option<B256>,
    pub receipts_root: Option<B256>,
    pub logs_bloom: Option<Bloom>,
    pub extra_data: Option<Bytes>,
    pub mix_hash: Option<B256>,
    pub total_difficulty: Option<U256>,
    pub base_fee_per_gas: Option<u128>,
    pub withdrawals_root: Option<B256>,
    pub blob_gas_used: Option<u128>,
    pub excess_blob_gas: Option<u128>,
    pub parent_beacon_block_root: Option<B256>,
}

impl Default for BlockQueryRes {
    fn default() -> Self {
        Self {
            number: None,
            timestamp: None,
            hash: None,
            size: None,
            parent_hash: None,
            state_root: None,
            transactions_root: None,
            receipts_root: None,
            logs_bloom: None,
            extra_data: None,
            mix_hash: None,
            total_difficulty: None,
            base_fee_per_gas: None,
            withdrawals_root: None,
            blob_gas_used: None,
            excess_blob_gas: None,
            parent_beacon_block_root: None,
        }
    }
}

#[serde_with::skip_serializing_none]
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Clone)]
pub struct AccountQueryRes {
    pub nonce: Option<u64>,
    #[serde(serialize_with = "serialize_option_u256")]
    pub balance: Option<U256>,
    pub address: Option<Address>,
    pub code: Option<Bytes>,
}

impl Default for AccountQueryRes {
    fn default() -> Self {
        Self {
            nonce: None,
            balance: None,
            address: None,
            code: None,
        }
    }
}

#[serde_with::skip_serializing_none]
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Clone)]
pub struct TransactionQueryRes {
    pub transaction_type: Option<u8>,
    pub hash: Option<FixedBytes<32>>,
    pub from: Option<Address>,
    pub to: Option<Address>,
    pub data: Option<Bytes>,
    #[serde(serialize_with = "serialize_option_u256")]
    pub value: Option<U256>,
    pub gas_price: Option<u128>,
    pub gas: Option<u128>,
    pub status: Option<bool>,
    pub chain_id: Option<u64>,
    pub v: Option<U256>,
    pub r: Option<U256>,
    pub s: Option<U256>,
    pub max_fee_per_blob_gas: Option<u128>,
    pub max_fee_per_gas: Option<u128>,
    pub max_priority_fee_per_gas: Option<u128>,
    pub y_parity: Option<bool>,
}

impl Default for TransactionQueryRes {
    fn default() -> Self {
        Self {
            transaction_type: None,
            hash: None,
            from: None,
            to: None,
            data: None,
            value: None,
            gas_price: None,
            gas: None,
            status: None,
            chain_id: None,
            v: None,
            r: None,
            s: None,
            max_fee_per_blob_gas: None,
            max_fee_per_gas: None,
            max_priority_fee_per_gas: None,
            y_parity: None,
        }
    }
}

#[cfg(test)]
mod test {
    use std::str::FromStr;

    use super::serialize_option_u256;
    use alloy::primitives::U256;
    use serde::Serialize;
    use serde_json::json;

    #[derive(Serialize)]
    struct U256Serializable {
        #[serde(serialize_with = "serialize_option_u256")]
        pub value: Option<U256>,
    }

    #[test]
    fn test_u256_serialization() {
        let value = U256::from_str("100").expect("Unable to parse value to U256");
        let u256 = U256Serializable { value: Some(value) };
        let u256_str = json!(u256).to_string();
        assert_eq!("{\"value\":\"100\"}", u256_str);
    }
}

fn serialize_option_u256<S>(option: &Option<U256>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match option {
        Some(u256) => serializer.serialize_some(&u256.to_string()),
        None => serializer.serialize_none(),
    }
}

// TODO: write test
pub fn dump_results(result: &ExpressionResult, dump_file: &Dump) {
    match dump_file.format {
        DumpFormat::Json => {
            dump_json(result, dump_file);
        }
        DumpFormat::Csv => {
            dump_csv(result, dump_file);
        }
        DumpFormat::Parquet => {
            dump_parquet(result, dump_file);
        }
    }
}

// TODO: write test
fn dump_json(result: &ExpressionResult, dump_file: &Dump) {
    let mut content: String = "[".to_string();

    match result {
        ExpressionResult::Account(account_results) => {
            for account_result in account_results.iter() {
                content.push_str(&serde_json::to_string(account_result).unwrap());
                content.push_str(",");
            }
        }
        ExpressionResult::Block(block_results) => {
            for block_result in block_results.iter() {
                content.push_str(&serde_json::to_string(block_result).unwrap());
                content.push_str(",");
            }
        }
        ExpressionResult::Transaction(transaction_results) => {
            for transaction_result in transaction_results.iter() {
                content.push_str(&serde_json::to_string(transaction_result).unwrap());
                content.push_str(",");
            }
        }
    }

    content.push_str("]");

    std::fs::write(&dump_file.path(), content).expect("Unable to write to file");
}

fn dump_csv(result: &ExpressionResult, dump_file: &Dump) {
    let mut writer = Writer::from_path(&dump_file.path()).expect("Unable to create CSV writer");

    match result {
        ExpressionResult::Account(account_results) => {
            for account_result in account_results.iter() {
                writer
                    .serialize(account_result)
                    .expect("Unable to serialize account result");
            }
        }
        ExpressionResult::Block(block_results) => {
            for block_result in block_results.iter() {
                writer
                    .serialize(block_result)
                    .expect("Unable to serialize block result");
            }
        }
        ExpressionResult::Transaction(transaction_results) => {
            for transaction_result in transaction_results.iter() {
                writer
                    .serialize(transaction_result)
                    .expect("Unable to serialize transaction result");
            }
        }
    }

    writer.flush().expect("Unable to flush CSV writer");
}

fn dump_parquet(_results: &ExpressionResult, _dump_file: &Dump) {
    unimplemented!();
}
