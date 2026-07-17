use super::resolve_portal::{
    portal_query, value_to_address, value_to_b256, value_to_bytes, value_to_u64,
};
use crate::common::{
    chain::{Chain, ChainOrRpc},
    logs::{LogField, LogFilter, Logs},
    query_result::LogQueryRes,
};
use alloy::eips::BlockNumberOrTag;
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

/// Returns true if a LogField can be served by the SQD Portal.
fn field_supported_by_portal(field: &LogField) -> bool {
    matches!(
        field,
        LogField::Address
            | LogField::Topic0
            | LogField::Topic1
            | LogField::Topic2
            | LogField::Topic3
            | LogField::Data
            | LogField::BlockNumber
            | LogField::BlockTimestamp
            | LogField::TransactionHash
            | LogField::TransactionIndex
            | LogField::LogIndex
            | LogField::Chain
    )
}

/// Returns true if a LogFilter is supported by Portal.
/// Portal does not support BlockHash or EventSignature filters.
fn filter_supported_by_portal(filter: &LogFilter) -> bool {
    matches!(
        filter,
        LogFilter::BlockRange(_)
            | LogFilter::EmitterAddress(_)
            | LogFilter::Topic0(_)
            | LogFilter::Topic1(_)
            | LogFilter::Topic2(_)
            | LogFilter::Topic3(_)
    )
}

/// Extract the block range from log filters. Returns None if no block range filter is found.
fn extract_block_range(filters: &[LogFilter]) -> Option<(u64, u64)> {
    for filter in filters {
        if let LogFilter::BlockRange(range) = filter {
            let start = match range.start() {
                BlockNumberOrTag::Number(n) => n,
                _ => return None,
            };
            let end = match range.end() {
                Some(BlockNumberOrTag::Number(n)) => n,
                Some(_) => return None,
                None => start,
            };
            return Some((start, end));
        }
    }
    None
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

    // All fields must be Portal-compatible
    if !logs.fields().iter().all(|f| field_supported_by_portal(f)) {
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

    // Must have a concrete block range
    extract_block_range(logs.filter()).is_some()
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

    let (from_block, to_block) = extract_block_range(filters).unwrap();

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
            LogFilter::BlockRange(_) => {} // Handled via fromBlock/toBlock
            _ => {}
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
            _ => {} // BlockNumber, BlockTimestamp, Chain handled above or client-side
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

        if let Some(portal_logs) = portal_block.get("logs").and_then(|l| l.as_array()) {
            for log in portal_logs {
                let result =
                    parse_portal_log(log, fields, &chain_enum, block_number, block_timestamp);
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
            LogField::Chain => {
                result.chain = Some(chain.clone());
            }
            _ => {} // BlockHash, Removed — not supported by Portal
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
