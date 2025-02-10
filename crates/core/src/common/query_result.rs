use crate::common::chain::Chain;
use alloy::primitives::{Address, Bloom, Bytes, FixedBytes, B256, U256};
use alloy_eip7702::SignedAuthorization;
use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize, Serializer};

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Clone)]
pub struct QueryResult {
    pub result: ExpressionResult,
}

impl QueryResult {
    pub fn new(result: ExpressionResult) -> QueryResult {
        QueryResult { result }
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
    #[serde(rename = "log")]
    Log(Vec<LogQueryRes>),
    #[serde(rename = "count")]
    Count(Vec<CountQueryRes>),

}

// TODO: should this be replaced with Alloy's Block?
#[serde_with::skip_serializing_none]
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Clone)]
pub struct BlockQueryRes {
    pub chain: Option<Chain>,
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
    pub base_fee_per_gas: Option<u64>,
    pub withdrawals_root: Option<B256>,
    pub blob_gas_used: Option<u64>,
    pub excess_blob_gas: Option<u64>,
    pub parent_beacon_block_root: Option<B256>,
}

impl Default for BlockQueryRes {
    fn default() -> Self {
        Self {
            chain: None,
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
    pub chain: Option<Chain>,
    pub nonce: Option<u64>,
    #[serde(serialize_with = "serialize_option_u256")]
    pub balance: Option<U256>,
    pub address: Option<Address>,
    pub code: Option<Bytes>,
}

impl Default for AccountQueryRes {
    fn default() -> Self {
        Self {
            chain: None,
            nonce: None,
            balance: None,
            address: None,
            code: None,
        }
    }
}

#[serde_with::skip_serializing_none]
#[derive(Debug, PartialEq, Eq, Deserialize, Clone)]
pub struct TransactionQueryRes {
    pub chain: Option<Chain>,
    pub r#type: Option<u8>,
    pub hash: Option<FixedBytes<32>>,
    pub from: Option<Address>,
    pub to: Option<Address>,
    pub data: Option<Bytes>,
    #[serde(serialize_with = "serialize_option_u256")]
    pub value: Option<U256>,
    pub gas_price: Option<u128>,
    pub gas_limit: Option<u64>,
    pub effective_gas_price: Option<u128>,
    pub status: Option<bool>,
    pub chain_id: Option<u64>,
    pub v: Option<bool>,
    pub r: Option<U256>,
    pub s: Option<U256>,
    pub max_fee_per_blob_gas: Option<u128>,
    pub max_fee_per_gas: Option<u128>,
    pub max_priority_fee_per_gas: Option<u128>,
    pub y_parity: Option<bool>,
    pub authorization_list: Option<Vec<SignedAuthorization>>,
}

impl Default for TransactionQueryRes {
    fn default() -> Self {
        Self {
            chain: None,
            r#type: None,
            hash: None,
            from: None,
            to: None,
            data: None,
            value: None,
            gas_price: None,
            gas_limit: None,
            status: None,
            chain_id: None,
            v: None,
            r: None,
            s: None,
            effective_gas_price: None,
            max_fee_per_blob_gas: None,
            max_fee_per_gas: None,
            max_priority_fee_per_gas: None,
            y_parity: None,
            authorization_list: None,
        }
    }
}

impl TransactionQueryRes {
    pub fn has_value(&self) -> bool {
        self.chain.is_some()
            || self.r#type.is_some()
            || self.hash.is_some()
            || self.from.is_some()
            || self.to.is_some()
            || self.data.is_some()
            || self.value.is_some()
            || self.gas_price.is_some()
            || self.gas_limit.is_some()
            || self.effective_gas_price.is_some()
            || self.status.is_some()
            || self.chain_id.is_some()
            || self.v.is_some()
            || self.r.is_some()
            || self.s.is_some()
            || self.max_fee_per_blob_gas.is_some()
            || self.max_fee_per_gas.is_some()
            || self.max_priority_fee_per_gas.is_some()
            || self.y_parity.is_some()
            || self.authorization_list.is_some()
    }

    fn get_field_values(&self) -> Vec<(&'static str, String)> {
        let mut fields = Vec::new();
        if let Some(chain) = &self.chain {
            fields.push(("chain", Some(chain.to_string())));
        }
        if let Some(r#type) = self.r#type {
            fields.push(("type", Some(r#type.to_string())));
        }
        if let Some(hash) = &self.hash {
            fields.push(("hash", Some(format!("{hash:?}"))));
        }
        if let Some(from) = &self.from {
            fields.push(("from", Some(from.to_string())));
        }
        if let Some(to) = &self.to {
            fields.push(("to", Some(to.to_string())));
        }
        if let Some(data) = &self.data {
            fields.push(("data", Some(format!("{data:?}"))));
        }
        if let Some(value) = &self.value {
            fields.push(("value", Some(value.to_string())));
        }
        if let Some(gas_price) = self.gas_price {
            fields.push(("gas_price", Some(gas_price.to_string())));
        }
        if let Some(gas_limit) = self.gas_limit {
            fields.push(("gas_limit", Some(gas_limit.to_string())));
        }
        if let Some(effective_gas_price) = self.effective_gas_price {
            fields.push(("effective_gas_price", Some(effective_gas_price.to_string())));
        }
        if let Some(status) = self.status {
            fields.push(("status", Some(status.to_string())));
        }
        if let Some(chain_id) = self.chain_id {
            fields.push(("chain_id", Some(chain_id.to_string())));
        }
        if let Some(v) = self.v {
            fields.push(("v", Some(v.to_string())));
        }
        if let Some(r) = &self.r {
            fields.push(("r", Some(r.to_string())));
        }
        if let Some(s) = &self.s {
            fields.push(("s", Some(s.to_string())));
        }
        if let Some(max_fee_per_blob_gas) = self.max_fee_per_blob_gas {
            fields.push((
                "max_fee_per_blob_gas",
                Some(max_fee_per_blob_gas.to_string()),
            ));
        }
        if let Some(max_fee_per_gas) = self.max_fee_per_gas {
            fields.push(("max_fee_per_gas", Some(max_fee_per_gas.to_string())));
        }
        if let Some(max_priority_fee_per_gas) = self.max_priority_fee_per_gas {
            fields.push((
                "max_priority_fee_per_gas",
                Some(max_priority_fee_per_gas.to_string()),
            ));
        }
        if let Some(y_parity) = self.y_parity {
            fields.push(("y_parity", Some(y_parity.to_string())));
        }

        if let Some(auths) = &self.authorization_list {
            for (i, auth) in auths.iter().enumerate() {
                fields.push((
                    Box::leak(format!("authorization_list_{i}_chain_id").into_boxed_str()),
                    Some(auth.chain_id.to_string()),
                ));
                fields.push((
                    Box::leak(format!("authorization_list_{i}_address").into_boxed_str()),
                    Some(auth.address.to_string()),
                ));
                fields.push((
                    Box::leak(format!("authorization_list_{i}_r").into_boxed_str()),
                    Some(format!("{:?}", auth.r())),
                ));
                fields.push((
                    Box::leak(format!("authorization_list_{i}_s").into_boxed_str()),
                    Some(format!("{:?}", auth.s())),
                ));
                fields.push((
                    Box::leak(format!("authorization_list_{i}_y_parity").into_boxed_str()),
                    Some(format!("{:?}", auth.y_parity())),
                ));
                fields.push((
                    Box::leak(format!("authorization_list_{i}_nonce").into_boxed_str()),
                    Some(auth.nonce.to_string()),
                ));
            }
        }

        fields
            .into_iter()
            .map(|(name, value)| (name, value.unwrap_or_default()))
            .collect()
    }
}

impl Serialize for TransactionQueryRes {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let fields = self.get_field_values();
        let mut state = serializer.serialize_struct("TransactionQueryRes", fields.len())?;
        for (field_name, value) in fields {
            state.serialize_field(field_name, &value)?;
        }
        state.end()
    }
}

#[serde_with::skip_serializing_none]
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Clone)]
pub struct LogQueryRes {
    pub chain: Option<Chain>,
    pub address: Option<Address>,
    pub topic0: Option<FixedBytes<32>>,
    pub topic1: Option<FixedBytes<32>>,
    pub topic2: Option<FixedBytes<32>>,
    pub topic3: Option<FixedBytes<32>>,
    pub data: Option<Bytes>,
    pub block_hash: Option<B256>,
    pub block_number: Option<u64>,
    pub block_timestamp: Option<u64>,
    pub transaction_hash: Option<B256>,
    pub transaction_index: Option<u64>,
    pub log_index: Option<u64>,
    pub removed: Option<bool>,
}

impl Default for LogQueryRes {
    fn default() -> Self {
        Self {
            chain: None,
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

#[serde_with::skip_serializing_none]
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Clone)]
pub struct CountQueryRes {
    pub count: usize,
}

impl Default for CountQueryRes {
    fn default() -> Self {
        Self {
            count:0
        }
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
