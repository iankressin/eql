use alloy::primitives::{Address, Bloom, Bytes, FixedBytes, B256, U256};
use serde::{Deserialize, Serialize, Serializer};
use tabled::Tabled;

fn serialize_option_u256<S>(option: &Option<U256>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match option {
        Some(u256) => serializer.serialize_some(&u256.to_string()),
        None => serializer.serialize_none(),
    }
}

// TODO: core structs shouldn't derive Tabled. It must be implemented on the CLI crate
// TODO: should this be replaced with Alloy's Block?
#[derive(Debug, PartialEq, Eq, Tabled, Serialize, Deserialize, Clone)]
pub struct BlockQueryRes {
    #[tabled(display_with = "display_option")]
    pub number: Option<u64>,
    #[tabled(display_with = "display_option")]
    pub timestamp: Option<u64>,
    #[tabled(display_with = "display_option")]
    pub hash: Option<B256>,
    #[tabled(display_with = "display_option")]
    #[serde(serialize_with = "serialize_option_u256")]
    pub size: Option<U256>,
    #[tabled(display_with = "display_option")]
    pub parent_hash: Option<B256>,
    #[tabled(display_with = "display_option")]
    pub state_root: Option<B256>,
    #[tabled(display_with = "display_option")]
    pub transactions_root: Option<B256>,
    #[tabled(display_with = "display_option")]
    pub receipts_root: Option<B256>,
    #[tabled(display_with = "display_option")]
    pub logs_bloom: Option<Bloom>,
    #[tabled(display_with = "display_option")]
    pub extra_data: Option<Bytes>,
    #[tabled(display_with = "display_option")]
    pub mix_hash: Option<B256>,
    #[tabled(display_with = "display_option")]
    pub total_difficulty: Option<U256>,
    #[tabled(display_with = "display_option")]
    pub base_fee_per_gas: Option<u128>,
    #[tabled(display_with = "display_option")]
    pub withdrawals_root: Option<B256>,
    #[tabled(display_with = "display_option")]
    pub blob_gas_used: Option<u128>,
    #[tabled(display_with = "display_option")]
    pub excess_blob_gas: Option<u128>,
    #[tabled(display_with = "display_option")]
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

// TODO: core structs shouldn't derive Tabled. It must be implemented on the CLI crate
#[derive(Debug, PartialEq, Eq, Tabled, Serialize, Deserialize, Clone)]
pub struct AccountQueryRes {
    #[tabled(display_with = "display_option")]
    pub nonce: Option<u64>,
    #[tabled(display_with = "display_option")]
    #[serde(serialize_with = "serialize_option_u256")]
    pub balance: Option<U256>,
    #[tabled(display_with = "display_option")]
    pub address: Option<Address>,
    #[tabled(display_with = "display_option")]
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

// TODO: core structs shouldn't derive Tabled. It must be implemented on the CLI crate
#[derive(Debug, PartialEq, Eq, Tabled, Serialize, Deserialize, Clone)]
pub struct TransactionQueryRes {
    #[tabled(display_with = "display_option")]
    pub transaction_type: Option<u8>,
    #[tabled(display_with = "display_option")]
    pub hash: Option<FixedBytes<32>>,
    #[tabled(display_with = "display_option")]
    pub from: Option<Address>,
    #[tabled(display_with = "display_option")]
    pub to: Option<Address>,
    #[tabled(display_with = "display_option")]
    pub data: Option<Bytes>,
    #[tabled(display_with = "display_option")]
    #[serde(serialize_with = "serialize_option_u256")]
    pub value: Option<U256>,
    #[tabled(display_with = "display_option")]
    pub gas_price: Option<u128>,
    #[tabled(display_with = "display_option")]
    pub gas: Option<u128>,
    #[tabled(display_with = "display_option")]
    pub status: Option<bool>,
    #[tabled(display_with = "display_option")]
    pub chain_id: Option<u64>,
    #[tabled(display_with = "display_option")]
    pub v: Option<U256>,
    #[tabled(display_with = "display_option")]
    pub r: Option<U256>,
    #[tabled(display_with = "display_option")]
    pub s: Option<U256>,
    #[tabled(display_with = "display_option")]
    pub max_fee_per_blob_gas: Option<u128>,
    #[tabled(display_with = "display_option")]
    pub max_fee_per_gas: Option<u128>,
    #[tabled(display_with = "display_option")]
    pub max_priority_fee_per_gas: Option<u128>,
    #[tabled(display_with = "display_option")]
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

// TODO: core structs shouldn't derive Tabled. It must be implemented on the CLI crate
#[derive(Debug, PartialEq, Eq, Tabled, Serialize, Deserialize, Clone)]
pub struct LogQueryRes {
    #[tabled(display_with = "display_option")]
    pub address: Option<Address>,
    #[tabled(display_with = "display_option")]
    pub topic0: Option<FixedBytes<32>>,
    #[tabled(display_with = "display_option")]
    pub topic1: Option<FixedBytes<32>>,
    #[tabled(display_with = "display_option")]
    pub topic2: Option<FixedBytes<32>>,
    #[tabled(display_with = "display_option")]
    pub topic3: Option<FixedBytes<32>>,
    #[tabled(display_with = "display_option")]
    pub data: Option<Bytes>,
    #[tabled(display_with = "display_option")]
    pub block_hash: Option<B256>,
    #[tabled(display_with = "display_option")]
    pub block_number: Option<u64>,
    #[tabled(display_with = "display_option")]
    pub block_timestamp: Option<u64>,
    #[tabled(display_with = "display_option")]
    pub transaction_hash: Option<B256>,
    #[tabled(display_with = "display_option")]
    pub transaction_index: Option<u64>,
    #[tabled(display_with = "display_option")]
    pub log_index: Option<u64>,
    #[tabled(display_with = "display_option")]
    pub removed: Option<bool>,
}

impl Default for LogQueryRes {
    fn default() -> Self {
        Self {
            address: None,
            topic0: None,
            topic1: None,
            topic2: None,
            topic3: None,
            data: None,
            block_hash: None,
            block_number: None,
            block_timestamp: None,
            transaction_hash: None,
            transaction_index: None,
            log_index: None,
            removed: None,
        }
    }
}

// TODO: move to another file
fn display_option<T: std::fmt::Display>(value: &Option<T>) -> String {
    match value {
        Some(value) => value.to_string(),
        None => "-".to_string(),
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
