use crate::common::{
    block::{get_block_number_from_tag, Block, BlockField, BlockId},
    chain::{Chain, ChainOrRpc},
    query_result::BlockQueryRes,
};
use alloy::{
    eips::BlockNumberOrTag,
    providers::{Provider, ProviderBuilder, RootProvider},
    rpc::types::{Block as RpcBlock, BlockTransactionsKind},
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
    #[error("Mismatch between Entity and EntityId, {0} can't be resolved as a block id")]
    MismatchEntityAndEntityId(String),
    #[error("Missing block ids")]
    IdsNotSet,
}

async fn resolve_block_id(
    id: &BlockId,
    provider: Arc<RootProvider<Http<Client>>>,
) -> Result<Vec<u64>> {
    let block_numbers = match id {
        BlockId::Range(block_range) => block_range.resolve_block_numbers(&provider).await?,
        BlockId::Number(block_number) => {
            resolve_block_numbers(&[block_number.clone()], provider.clone()).await?
        }
    };

    Ok(block_numbers)
}

pub async fn resolve_block_query(
    block: &Block,
    chains: &[ChainOrRpc],
) -> Result<Vec<BlockQueryRes>> {
    let mut all_chain_futures = Vec::new();

    let ids = match block.ids() {
        Some(ids) => ids,
        None => return Err(BlockResolverErrors::IdsNotSet.into()),
    };

    for chain in chains {
        let fields = block.fields().clone();

        let chain_future = async move {
            let provider = Arc::new(ProviderBuilder::new().on_http(chain.rpc_url()?));
            let chain = chain.to_chain().await?;
            let mut all_block_futures = Vec::new();

            for id in ids {
                let provider_clone = provider.clone();
                let chain_clone = chain.clone();
                let fields = fields.clone();

                let block_id = resolve_block_id(&id, provider_clone.clone()).await?;
                let block_future = async move {
                    get_filtered_blocks(block_id, fields, &provider_clone, &chain_clone).await
                };
                all_block_futures.push(block_future);
            }

            let chain_blocks = try_join_all(all_block_futures).await?;
            Ok::<Vec<BlockQueryRes>, anyhow::Error>(chain_blocks.concat())
        };

        all_chain_futures.push(chain_future);
    }

    let all_chain_blocks = try_join_all(all_chain_futures).await?;
    Ok(all_chain_blocks.concat())
}

async fn get_filtered_blocks(
    block_numbers: Vec<u64>,
    fields: Vec<BlockField>,
    provider: &Arc<RootProvider<Http<Client>>>,
    chain: &Chain,
) -> Result<Vec<BlockQueryRes>> {
    let blocks = batch_get_blocks(block_numbers, &provider, false).await?;
    Ok(blocks
        .into_iter()
        .map(|block| filter_fields(block, &fields, &chain))
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
            async move { get_block_number_from_tag(provider, block_number).await };
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
    let kind = if hydrate {
        BlockTransactionsKind::Full
    } else {
        BlockTransactionsKind::Hashes
    };

    match provider.get_block_by_number(block_id, kind).await? {
        Some(block) => Ok(block),
        None => return Err(BlockResolverErrors::UnableToFetchBlockNumber(block_id.clone()).into()),
    }
}

fn filter_fields(block: RpcBlock, fields: &[BlockField], chain: &Chain) -> BlockQueryRes {
    let mut result = BlockQueryRes::default();

    for field in fields {
        match field {
            BlockField::Timestamp => {
                result.timestamp = Some(block.header.timestamp);
            }
            BlockField::Number => {
                result.number = Some(block.header.number);
            }
            BlockField::Hash => {
                result.hash = Some(block.header.hash);
            }
            BlockField::ParentHash => {
                result.parent_hash = Some(block.header.parent_hash);
            }
            BlockField::Size => {
                result.size = block.header.size;
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
                result.mix_hash = Some(block.header.mix_hash);
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
            BlockField::Chain => {
                result.chain = Some(chain.clone());
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::{block::BlockRange, chain::Chain};

    #[tokio::test]
    async fn test_error_when_start_block_is_greater_than_end_block() {
        let start_block = 10;
        let end_block = 5;
        // Empty fields for simplicity
        let fields = vec![];
        let chain = ChainOrRpc::Chain(Chain::Ethereum);
        let block = Block::new(
            Some(vec![BlockId::Range(BlockRange::new(
                start_block.into(),
                Some(end_block.into()),
            ))]),
            None,
            fields,
        );

        let result = resolve_block_query(&block, &[chain]).await;
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Start block must be less than end block"
        );
    }
}
