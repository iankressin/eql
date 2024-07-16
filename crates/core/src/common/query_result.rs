use alloy::primitives::{Address, Bytes, FixedBytes, Uint, B256, U256};
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

#[derive(Debug, PartialEq, Eq, Tabled, Serialize, Deserialize)]
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
}

impl Default for BlockQueryRes {
    fn default() -> Self {
        Self {
            number: None,
            timestamp: None,
            hash: None,
            size: None,
            parent_hash: None,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Tabled, Serialize, Deserialize)]
pub struct AccountQueryRes {
    #[tabled(display_with = "display_option")]
    pub nonce: Option<u64>,
    #[tabled(display_with = "display_option")]
    #[serde(serialize_with = "serialize_option_u256")]
    pub balance: Option<U256>,
    #[tabled(display_with = "display_option")]
    pub address: Option<Address>,
}

impl Default for AccountQueryRes {
    fn default() -> Self {
        Self {
            nonce: None,
            balance: None,
            address: None,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Tabled, Serialize, Deserialize)]
pub struct TransactionQueryRes {
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
    pub status: Option<bool>,
}

impl Default for TransactionQueryRes {
    fn default() -> Self {
        Self {
            hash: None,
            from: None,
            to: None,
            data: None,
            value: None,
            gas_price: None,
            status: None,
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
