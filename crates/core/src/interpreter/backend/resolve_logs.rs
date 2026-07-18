use super::resolve_portal::{
    block_range_is_portal_eligible, portal_query, resolve_portal_bound, value_to_address,
    value_to_b256, value_to_bytes, value_to_u64,
};
use crate::common::{
    block::BlockRange,
    chain::{Chain, ChainOrRpc},
    logs::{LogField, LogFilter, Logs},
    query_result::LogQueryRes,
};
use alloy::primitives::keccak256;
use alloy::providers::{Provider, ProviderBuilder};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize, thiserror::Error)]
pub enum LogResolverErrors {
    #[error("Query returned no results within the given filters")]
    NoLogsFound,
}

/// Returns true if a LogFilter is supported by Portal.
/// Portal does not support BlockHash filters.
fn filter_supported_by_portal(filter: &LogFilter) -> bool {
    matches!(
        filter,
        LogFilter::BlockRange(_)
            | LogFilter::EmitterAddress(_)
            | LogFilter::EventSignature(_)
            | LogFilter::Topic0(_)
            | LogFilter::Topic1(_)
            | LogFilter::Topic2(_)
            | LogFilter::Topic3(_)
    )
}

/// Find the BlockRange filter, if present.
fn find_block_range(filters: &[LogFilter]) -> Option<&BlockRange> {
    filters.iter().find_map(|f| match f {
        LogFilter::BlockRange(range) => Some(range),
        _ => None,
    })
}

/// Determines if a log query for a given chain should use the Portal.
fn should_use_portal(chain: &ChainOrRpc, logs: &Logs) -> bool {
    let dataset = match chain {
        ChainOrRpc::Chain(c) => c.portal_dataset(),
        ChainOrRpc::Rpc(_) => None,
    };

    if dataset.is_none() {
        return false;
    }

    // All filters must be Portal-compatible
    if !logs
        .filter()
        .iter()
        .all(|f| filter_supported_by_portal(f))
    {
        return false;
    }

    // Must have a Portal-resolvable block range.
    match find_block_range(logs.filter()) {
        Some(range) => block_range_is_portal_eligible(range),
        None => false,
    }
}

pub async fn resolve_log_query(
    logs: &Logs,
    chain_or_rpcs: &[ChainOrRpc],
) -> Result<Vec<LogQueryRes>> {
    let mut all_results = Vec::new();

    for chain_or_rpc in chain_or_rpcs {
        let results = if should_use_portal(chain_or_rpc, logs) {
            resolve_logs_via_portal(logs, chain_or_rpc).await?
        } else {
            resolve_logs_via_rpc(logs, chain_or_rpc).await?
        };
        all_results.extend(results);
    }

    if all_results.is_empty() {
        return Err(LogResolverErrors::NoLogsFound.into());
    }

    Ok(all_results)
}

// ---------------------------------------------------------------------------
// Portal path
// ---------------------------------------------------------------------------

async fn resolve_logs_via_portal(
    logs: &Logs,
    chain_or_rpc: &ChainOrRpc,
) -> Result<Vec<LogQueryRes>> {
    let chain_enum = match chain_or_rpc {
        ChainOrRpc::Chain(c) => c.clone(),
        _ => unreachable!("should_use_portal guards against Rpc variant"),
    };
    let dataset = chain_enum.portal_dataset().unwrap();
    let fields = logs.fields();
    let filters = logs.filter();

    let range = find_block_range(filters).expect("should_use_portal guarantees a block range");
    let from_block = resolve_portal_bound(dataset, &range.start()).await?;
    let to_block = match range.end() {
        Some(end) => resolve_portal_bound(dataset, &end).await?,
        None => from_block,
    };

    // Build log filter object for Portal
    let mut log_filter = serde_json::Map::new();
    for filter in filters {
        match filter {
            LogFilter::EmitterAddress(addr) => {
                log_filter.insert(
                    "address".into(),
                    json!([format!("{:?}", addr)]),
                );
            }
            LogFilter::Topic0(topic) => {
                log_filter.insert("topic0".into(), json!([format!("{:?}", topic)]));
            }
            LogFilter::Topic1(topic) => {
                log_filter.insert("topic1".into(), json!([format!("{:?}", topic)]));
            }
            LogFilter::Topic2(topic) => {
                log_filter.insert("topic2".into(), json!([format!("{:?}", topic)]));
            }
            LogFilter::Topic3(topic) => {
                log_filter.insert("topic3".into(), json!([format!("{:?}", topic)]));
            }
            LogFilter::EventSignature(sig) => {
                let topic0 = keccak256(sig.as_bytes());
                log_filter.insert("topic0".into(), json!([format!("{:?}", topic0)]));
            }
            LogFilter::BlockRange(_) => {} // Handled via fromBlock/toBlock
            LogFilter::BlockHash(_) => {}  // unreachable: gate excludes block_hash filter
        }
    }

    // Build field selection
    let mut log_fields = serde_json::Map::new();
    let mut block_fields = serde_json::Map::new();
    let needs_block_number = fields.iter().any(|f| matches!(f, LogField::BlockNumber));
    let needs_block_timestamp = fields.iter().any(|f| matches!(f, LogField::BlockTimestamp));

    if needs_block_number {
        block_fields.insert("number".into(), json!(true));
    }
    if needs_block_timestamp {
        block_fields.insert("timestamp".into(), json!(true));
    }
    let needs_block_hash = fields.iter().any(|f| matches!(f, LogField::BlockHash));
    if needs_block_hash {
        block_fields.insert("hash".into(), json!(true));
    }

    for field in fields {
        match field {
            LogField::Address => {
                log_fields.insert("address".into(), json!(true));
            }
            LogField::Topic0 | LogField::Topic1 | LogField::Topic2 | LogField::Topic3 => {
                log_fields.insert("topics".into(), json!(true));
            }
            LogField::Data => {
                log_fields.insert("data".into(), json!(true));
            }
            LogField::TransactionHash => {
                log_fields.insert("transactionHash".into(), json!(true));
            }
            LogField::TransactionIndex => {
                log_fields.insert("transactionIndex".into(), json!(true));
            }
            LogField::LogIndex => {
                log_fields.insert("logIndex".into(), json!(true));
            }
            LogField::BlockHash
            | LogField::BlockNumber
            | LogField::BlockTimestamp
            | LogField::Removed
            | LogField::Chain => {}
        }
    }

    let mut fields_obj = serde_json::Map::new();
    if !log_fields.is_empty() {
        fields_obj.insert("log".into(), serde_json::Value::Object(log_fields));
    }
    if !block_fields.is_empty() {
        fields_obj.insert("block".into(), serde_json::Value::Object(block_fields));
    }

    let query = json!({
        "type": "evm",
        "fromBlock": from_block,
        "toBlock": to_block,
        "fields": fields_obj,
        "logs": [log_filter]
    });

    let response = portal_query(dataset, &query).await?;

    let mut results = Vec::new();
    for portal_block in &response {
        let header = portal_block.get("header");
        let block_number = header.and_then(|h| h.get("number")).and_then(value_to_u64);
        let block_timestamp = header
            .and_then(|h| h.get("timestamp"))
            .and_then(value_to_u64);
        let block_hash = header.and_then(|h| h.get("hash")).and_then(value_to_b256);

        if let Some(portal_logs) = portal_block.get("logs").and_then(|l| l.as_array()) {
            for log in portal_logs {
                let result = parse_portal_log(
                    log,
                    fields,
                    &chain_enum,
                    block_number,
                    block_timestamp,
                    block_hash,
                );
                results.push(result);
            }
        }
    }

    Ok(results)
}

fn parse_portal_log(
    log: &serde_json::Value,
    fields: &[LogField],
    chain: &Chain,
    block_number: Option<u64>,
    block_timestamp: Option<u64>,
    block_hash: Option<alloy::primitives::B256>,
) -> LogQueryRes {
    let mut result = LogQueryRes::default();

    let topics = log.get("topics").and_then(|t| t.as_array());

    for field in fields {
        match field {
            LogField::Address => {
                result.address = log.get("address").and_then(value_to_address);
            }
            LogField::Topic0 => {
                result.topic0 = topics
                    .and_then(|t| t.get(0))
                    .and_then(value_to_b256);
            }
            LogField::Topic1 => {
                result.topic1 = topics
                    .and_then(|t| t.get(1))
                    .and_then(value_to_b256);
            }
            LogField::Topic2 => {
                result.topic2 = topics
                    .and_then(|t| t.get(2))
                    .and_then(value_to_b256);
            }
            LogField::Topic3 => {
                result.topic3 = topics
                    .and_then(|t| t.get(3))
                    .and_then(value_to_b256);
            }
            LogField::Data => {
                result.data = log.get("data").and_then(value_to_bytes);
            }
            LogField::BlockHash => {
                result.block_hash = block_hash;
            }
            LogField::BlockNumber => {
                result.block_number = block_number;
            }
            LogField::BlockTimestamp => {
                result.block_timestamp = block_timestamp;
            }
            LogField::TransactionHash => {
                result.transaction_hash = log.get("transactionHash").and_then(value_to_b256);
            }
            LogField::TransactionIndex => {
                result.transaction_index = log.get("transactionIndex").and_then(value_to_u64);
            }
            LogField::LogIndex => {
                result.log_index = log.get("logIndex").and_then(value_to_u64);
            }
            LogField::Removed => {
                result.removed = Some(false);
            }
            LogField::Chain => {
                result.chain = Some(chain.clone());
            }
        }
    }

    result
}

// ---------------------------------------------------------------------------
// RPC path (original logic, extracted)
// ---------------------------------------------------------------------------

async fn resolve_logs_via_rpc(
    logs: &Logs,
    chain_or_rpc: &ChainOrRpc,
) -> Result<Vec<LogQueryRes>> {
    let provider = Arc::new(ProviderBuilder::new().on_http(chain_or_rpc.rpc_url()?));
    let filtered_logs = provider.get_logs(&logs.build_bloom_filter()).await?;
    let chain = chain_or_rpc.to_chain().await?;

    let results: Vec<LogQueryRes> = filtered_logs
        .into_iter()
        .map(|log| {
            let mut result = LogQueryRes::default();

            for field in logs.fields() {
                match field {
                    LogField::Address => result.address = Some(log.inner.address),
                    LogField::Topic0 => result.topic0 = log.topic0().copied(),
                    LogField::Topic1 => {
                        result.topic1 = log.inner.data.topics().get(1).copied()
                    }
                    LogField::Topic2 => {
                        result.topic2 = log.inner.data.topics().get(2).copied()
                    }
                    LogField::Topic3 => {
                        result.topic3 = log.inner.data.topics().get(3).copied()
                    }
                    LogField::Data => result.data = Some(log.data().data.clone()),
                    LogField::BlockHash => result.block_hash = log.block_hash,
                    LogField::BlockNumber => result.block_number = log.block_number,
                    LogField::BlockTimestamp => {
                        result.block_timestamp = log.block_timestamp
                    }
                    LogField::TransactionHash => {
                        result.transaction_hash = log.transaction_hash
                    }
                    LogField::TransactionIndex => {
                        result.transaction_index = log.transaction_index
                    }
                    LogField::LogIndex => result.log_index = log.log_index,
                    LogField::Removed => result.removed = Some(log.removed),
                    LogField::Chain => result.chain = Some(chain.clone()),
                }
            }

            result
        })
        .collect();

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::chain::Chain;
    use alloy::primitives::b256;
    use serde_json::json;

    #[test]
    fn test_parse_portal_log_sets_block_hash_and_removed() {
        let log = json!({
            "logIndex": 5,
            "transactionIndex": 9,
            "address": "0xdac17f958d2ee523a2206206994597c13d831ec7",
            "topics": ["0xcb8241adb0c3fdb35b70c24ce35c5eb0c17af7431c99f827d44a445ca624176a"]
        });
        let fields = vec![LogField::BlockHash, LogField::Removed, LogField::LogIndex];
        let block_hash = Some(b256!(
            "d34e3b2957865fe76c73ec91d798f78de95f2b0e0cddfc47e341b5f235dc4d58"
        ));
        let res = parse_portal_log(
            &log,
            &fields,
            &Chain::Ethereum,
            Some(4638757),
            Some(1511886266),
            block_hash,
        );
        assert_eq!(res.block_hash, block_hash);
        assert_eq!(res.removed, Some(false));
        assert_eq!(res.log_index, Some(5));
    }

    #[test]
    fn test_event_signature_filter_is_portal_supported() {
        assert!(filter_supported_by_portal(&LogFilter::EventSignature(
            "Transfer(address,address,uint256)".to_string()
        )));
        // block_hash filter is NOT Portal-serviceable.
        assert!(!filter_supported_by_portal(&LogFilter::BlockHash(
            alloy::primitives::B256::ZERO
        )));
    }
}
