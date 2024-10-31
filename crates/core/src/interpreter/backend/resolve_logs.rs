use crate::common::{
    chain::ChainOrRpc,
    logs::{LogField, Logs},
    query_result::LogQueryRes,
};
use alloy::providers::{Provider, ProviderBuilder};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize, thiserror::Error)]
pub enum LogResolverErrors {
    #[error("Query returned no results within the given filters")]
    NoLogsFound,
}

pub async fn resolve_log_query(
    logs: &Logs,
    chain_or_rpcs: &[ChainOrRpc],
) -> Result<Vec<LogQueryRes>> {
    let mut all_results = Vec::new();

    for chain_or_rpc in chain_or_rpcs {
        let provider = Arc::new(ProviderBuilder::new().on_http(chain_or_rpc.rpc_url()?));
        let filtered_logs = provider.get_logs(&logs.build_bloom_filter()).await?;
        let chain = chain_or_rpc.to_chain().await?;

        if !filtered_logs.is_empty() {
            let chain_results: Vec<LogQueryRes> = filtered_logs
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

            all_results.extend(chain_results);
        }
    }

    if all_results.is_empty() {
        return Err(LogResolverErrors::NoLogsFound.into());
    }

    Ok(all_results)
}
