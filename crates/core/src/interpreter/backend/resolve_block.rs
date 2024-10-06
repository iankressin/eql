use crate::common::{entity_id::EntityId, query_result::BlockQueryRes, types::BlockField};
use alloy::{
    eips::BlockNumberOrTag,
    providers::{Provider, RootProvider},
    transports::http::{Client, Http},
};
use futures::future::try_join_all;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize, thiserror::Error)]
pub enum BlockResolverErrors {
    #[error("Unable to fetch block number for tag {0}")]
    UnableToFetchBlockNumber(BlockNumberOrTag),
    #[error("Start block must be greater than end block")]
    StartBlockMustBeGreaterThanEndBlock,
    #[error("Mismatch between Entity and EntityId, {0} can't be resolved as a block id")]
    MismatchEntityAndEntityId(String),
}

/// Resolve the query to get blocks after receiving a block entity expression.
/// Iterate through entity_ids and map them to a futures list. Execute all futures concurrently and collect the results, flattening them into a single vec.
pub async fn resolve_block_query(
    entity_ids: Vec<EntityId>,
    fields: Vec<BlockField>,
    provider: Arc<RootProvider<Http<Client>>>,
) -> Result<Vec<BlockQueryRes>, Box<dyn Error>> {
    let mut block_futures = Vec::new();

    for entity_id in entity_ids {
        let fields = fields.clone();
        let provider = Arc::clone(&provider);
        let block_future = async move {
            match entity_id {
                EntityId::Block(block_range) => {
                    let (start_block, end_block) = block_range.range();
                    let start_block_number =
                        get_block_number_from_tag(provider.clone(), start_block).await?;
                    let end_block_number = match end_block {
                        Some(end) => Some(get_block_number_from_tag(provider.clone(), end).await?),
                        None => None,
                    };

                    if let Some(end_number) = end_block_number {
                        if start_block_number > end_number {
                            return Err(
                                BlockResolverErrors::StartBlockMustBeGreaterThanEndBlock.into()
                            );
                        }

                        batch_get_block(start_block_number, end_number, fields, provider.clone())
                            .await
                    } else {
                        batch_get_block(
                            start_block_number,
                            start_block_number,
                            fields,
                            provider.clone(),
                        )
                        .await
                    }
                }
                id => Err(Box::new(BlockResolverErrors::MismatchEntityAndEntityId(
                    id.to_string(),
                ))
                .into()),
            }
        };

        block_futures.push(block_future);
    }

    let block_res: Vec<Vec<BlockQueryRes>> = try_join_all(block_futures).await?;
    Ok(block_res.into_iter().flatten().collect())
}

async fn batch_get_block(
    start_block: u64,
    end_block: u64,
    fields: Vec<BlockField>,
    provider: Arc<RootProvider<Http<Client>>>,
) -> Result<Vec<BlockQueryRes>, Box<dyn Error>> {
    let mut block_futures = Vec::new();

    for block_number in start_block..=end_block {
        let fields = fields.clone();
        let provider = Arc::clone(&provider);

        let block_future = async move {
            get_block(BlockNumberOrTag::Number(block_number), fields, provider).await
        };

        block_futures.push(block_future);
    }

    let block_results = try_join_all(block_futures).await?;
    Ok(block_results)
}

async fn get_block(
    block_id: BlockNumberOrTag,
    fields: Vec<BlockField>,
    provider: Arc<RootProvider<Http<Client>>>,
) -> Result<BlockQueryRes, Box<dyn Error>> {
    let mut result = BlockQueryRes::default();

    match provider.get_block_by_number(block_id, false).await? {
        Some(block) => {
            for field in &fields {
                match field {
                    BlockField::Timestamp => {
                        result.timestamp = Some(block.header.timestamp);
                    }
                    BlockField::Number => {
                        result.number = block.header.number;
                    }
                    BlockField::Hash => {
                        result.hash = block.header.hash;
                    }
                    BlockField::ParentHash => {
                        result.parent_hash = Some(block.header.parent_hash);
                    }
                    BlockField::Size => {
                        result.size = block.size;
                    }
                    BlockField::StateRoot => {
                        result.state_root = Some(block.header.state_root);
                    }
                    BlockField::TransactionsRoot => {
                        result.transactions_root = Some(block.header.transactions_root);
                    }
                    BlockField::ReceiptsRoot => {
                        result.receipts_root = Some(block.header.receipts_root);
                    }
                    BlockField::LogsBloom => {
                        result.logs_bloom = Some(block.header.logs_bloom);
                    }
                    BlockField::ExtraData => {
                        result.extra_data = Some(block.header.extra_data.clone());
                    }
                    BlockField::MixHash => {
                        result.mix_hash = block.header.mix_hash;
                    }
                    BlockField::TotalDifficulty => {
                        result.total_difficulty = block.header.total_difficulty;
                    }
                    BlockField::BaseFeePerGas => {
                        result.base_fee_per_gas = block.header.base_fee_per_gas;
                    }
                    BlockField::WithdrawalsRoot => {
                        result.withdrawals_root = block.header.withdrawals_root;
                    }
                    BlockField::BlobGasUsed => {
                        result.blob_gas_used = block.header.blob_gas_used;
                    }
                    BlockField::ExcessBlobGas => {
                        result.excess_blob_gas = block.header.excess_blob_gas;
                    }
                    BlockField::ParentBeaconBlockRoot => {
                        result.parent_beacon_block_root = block.header.parent_beacon_block_root;
                    }
                }
            }
        }
        None => return Err(BlockResolverErrors::UnableToFetchBlockNumber(block_id).into()),
    }

    Ok(result)
}

async fn get_block_number_from_tag(
    provider: Arc<RootProvider<Http<Client>>>,
    number_or_tag: BlockNumberOrTag,
) -> Result<u64, Box<dyn Error>> {
    match number_or_tag {
        BlockNumberOrTag::Number(number) => Ok(number),
        block_tag => match provider.get_block_by_number(block_tag, false).await? {
            Some(block) => match block.header.number {
                Some(number) => Ok(number),
                None => Err(Box::new(BlockResolverErrors::UnableToFetchBlockNumber(
                    number_or_tag,
                ))),
            },
            None => Err(Box::new(BlockResolverErrors::UnableToFetchBlockNumber(
                number_or_tag,
            ))),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::entity_filter::BlockRange;
    use alloy::providers::ProviderBuilder;

    #[tokio::test]
    async fn test_resolve_block_query_when_start_is_greater_than_end() {
        let start_block = 10;
        let end_block = 5;
        // Empty fields for simplicity
        let fields = vec![];
        let provider = Arc::new(
            ProviderBuilder::new().on_http("https://rpc.ankr.com/eth_sepolia".parse().unwrap()),
        );
        let entity_id =
            EntityId::Block(BlockRange::new(start_block.into(), Some(end_block.into())));

        let result = resolve_block_query(vec![entity_id], fields, provider)
            .await
            .unwrap_err()
            .to_string();

        assert_eq!(result, "Start block must be greater than end block");
    }
}
