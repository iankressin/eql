use crate::common::{
    block::{Block, BlockField, BlockId},
    query_result::BlockQueryRes,
};
use alloy::{
    eips::BlockNumberOrTag,
    providers::{Provider, RootProvider},
    rpc::types::Block as RpcBlock,
    transports::http::{Client, Http},
};
use anyhow::Result;
use futures::future::try_join_all;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize, thiserror::Error)]
pub enum BlockResolverErrors {
    #[error("Unable to fetch block number for tag {0}")]
    UnableToFetchBlockNumber(BlockNumberOrTag),
    #[error("Start block must be greater than end block")]
    StartBlockMustBeGreaterThanEndBlock,
    #[error("Mismatch between Entity and EntityId, {0} can't be resolved as a block id")]
    MismatchEntityAndEntityId(String),
    #[error("Missing block ids")]
    IdsNotSet,
}

/// Resolve the query to get blocks after receiving a block entity expression.
/// Iterate through entity_ids and map them to a futures list. Execute all futures concurrently and collect the results, flattening them into a single vec.
pub async fn resolve_block_query(
    block: &Block,
    provider: Arc<RootProvider<Http<Client>>>,
) -> Result<Vec<BlockQueryRes>> {
    let mut block_futures = Vec::new();

    let ids = match block.ids() {
        Some(ids) => ids,
        None => return Err(BlockResolverErrors::IdsNotSet.into()),
    };

    for id in ids {
        let provider = Arc::clone(&provider);
        let fields = block.fields().clone();
        let block_future = async move {
            let block_numbers = match id {
                BlockId::Range(block_range) => block_range.resolve_block_numbers(&provider).await?,
                BlockId::Number(block_number) => {
                    resolve_block_numbers(&[block_number.clone()], provider.clone()).await?
                }
            };
            get_block_and_filter_fields(block_numbers, provider.clone(), fields.clone()).await
        };

        block_futures.push(block_future);
    }

    let block_res: Vec<Vec<BlockQueryRes>> = try_join_all(block_futures).await?;
    Ok(block_res.into_iter().flatten().collect())
}

async fn get_block_and_filter_fields(
    block_numbers: Vec<u64>,
    provider: Arc<RootProvider<Http<Client>>>,
    fields: Vec<BlockField>,
) -> Result<Vec<BlockQueryRes>> {
    let blocks = batch_get_blocks(block_numbers, &provider, false).await?;
    Ok(blocks
        .into_iter()
        .map(|block| filter_fields(block, fields.clone()))
        .collect())
}

// TODO: this method only exists here because it wasn't implemented on the BlockId struct yet.
// BlockRange has a similar implementation and should be unified.
async fn resolve_block_numbers(
    block_numbers: &[BlockNumberOrTag],
    provider: Arc<RootProvider<Http<Client>>>,
) -> Result<Vec<u64>> {
    let mut block_number_futures = Vec::new();

    for block_number in block_numbers {
        let provider = Arc::clone(&provider);
        let block_number_future =
            async move { get_block_number_from_tag(provider, block_number.clone()).await };
        block_number_futures.push(block_number_future);
    }

    let block_numbers = try_join_all(block_number_futures).await?;
    Ok(block_numbers)
}

pub async fn batch_get_blocks(
    block_numbers: Vec<u64>,
    provider: &Arc<RootProvider<Http<Client>>>,
    hydrate: bool,
) -> Result<Vec<RpcBlock>> {
    let mut block_futures = Vec::new();

    for block_number in block_numbers {
        let provider = Arc::clone(&provider);
        let block_future = async move {
            get_block(BlockNumberOrTag::Number(block_number), provider, hydrate).await
        };
        block_futures.push(block_future);
    }

    let block_results = try_join_all(block_futures).await?;
    Ok(block_results)
}

pub async fn get_block(
    block_id: BlockNumberOrTag,
    provider: Arc<RootProvider<Http<Client>>>,
    hydrate: bool,
) -> Result<RpcBlock> {
    match provider.get_block_by_number(block_id, hydrate).await? {
        Some(block) => Ok(block),
        None => return Err(BlockResolverErrors::UnableToFetchBlockNumber(block_id.clone()).into()),
    }
}

fn filter_fields(block: RpcBlock, fields: Vec<BlockField>) -> BlockQueryRes {
    let mut result = BlockQueryRes::default();

    for field in fields {
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

    result
}

async fn get_block_number_from_tag(
    provider: Arc<RootProvider<Http<Client>>>,
    number_or_tag: BlockNumberOrTag,
) -> Result<u64> {
    match number_or_tag {
        BlockNumberOrTag::Number(number) => Ok(number),
        block_tag => match provider.get_block_by_number(block_tag, false).await? {
            Some(block) => match block.header.number {
                Some(number) => Ok(number),
                None => Err(BlockResolverErrors::UnableToFetchBlockNumber(number_or_tag).into()),
            },
            None => Err(BlockResolverErrors::UnableToFetchBlockNumber(number_or_tag).into()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::block::BlockRange;
    use alloy::providers::ProviderBuilder;

    #[tokio::test]
    async fn test_error_when_start_block_is_greater_than_end_block() {
        let start_block = 10;
        let end_block = 5;
        // Empty fields for simplicity
        let fields = vec![];
        let provider = Arc::new(
            ProviderBuilder::new().on_http("https://rpc.ankr.com/eth_sepolia".parse().unwrap()),
        );
        let block = Block::new(
            Some(vec![BlockId::Range(BlockRange::new(
                start_block.into(),
                Some(end_block.into()),
            ))]),
            None,
            fields,
        );

        let result = resolve_block_query(&block, provider).await;
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Start block must be greater than end block"
        );
    }
}
