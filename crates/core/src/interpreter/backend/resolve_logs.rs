use crate::common::{query_result::LogQueryRes, types::LogField};
use std::error::Error;
use alloy::{
    providers::{Provider, RootProvider}, rpc::types::Filter, transports::http::{Client, Http}
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, thiserror::Error)]
pub enum LogResolverErrors {
    // #[error("Invalid address")]
    // InvalidAddress,
    #[error("Mismatch between Entity and EntityId")]
    MismatchEntityAndEntityId,
}

pub async fn resolve_log_query(
    filter: Filter, 
    fields: Vec<LogField>,
    provider: &RootProvider<Http<Client>>,
) -> Result<Vec<LogQueryRes>, Box<dyn Error>> {
    
    let logs = get_logs(filter, fields, &provider).await?;
    Ok(logs)
}

async fn get_logs(
    filter: Filter,
    fields: Vec<LogField>,
    provider: &RootProvider<Http<Client>>,
) -> Result<Vec<LogQueryRes>, Box<dyn Error>> {
    
    let logs = provider.get_logs(&filter).await?;
    if logs.is_empty() {
        return Err("No logs found".into()); // Check if this is the best approach for no logs return. I understand it shouldn't panic.
    }

    let results: Vec<LogQueryRes> = logs.into_iter().map(|log| {
        let mut result = LogQueryRes::default();

        // Use a for loop with match to map fields to result
        for field in &fields {
            match field {
                LogField::Address => result.address = Some(log.inner.address),
                LogField::Topic0 => result.topic0 = log.topic0().copied(),
                LogField::Topic1 => result.topic1 = log.inner.data.topics().get(1).copied(),
                LogField::Topic2 => result.topic2 = log.inner.data.topics().get(2).copied(),
                LogField::Topic3 => result.topic3 = log.inner.data.topics().get(3).copied(),
                LogField::Data => result.data = Some(log.data().data.clone()),
                LogField::BlockHash => result.block_hash = log.block_hash,
                LogField::BlockNumber => result.block_number = log.block_number,
                LogField::BlockTimestamp => result.block_timestamp = log.block_timestamp,
                LogField::TransactionHash => result.transaction_hash = log.transaction_hash,
                LogField::TransactionIndex => result.transaction_index = log.transaction_index,
                LogField::LogIndex => result.log_index = log.log_index,
                LogField::Removed => result.removed = Some(log.removed),
            }
        }

        result
    }).collect();

    Ok(results)
}