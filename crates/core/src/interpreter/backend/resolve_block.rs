use super::resolve_portal::{portal_query, value_to_b256, value_to_u64};
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
use serde_json::json;
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

/// Returns true if a BlockField can be served by the SQD Portal.
fn field_supported_by_portal(field: &BlockField) -> bool {
    matches!(
        field,
        BlockField::Number
            | BlockField::Timestamp
            | BlockField::Hash
            | BlockField::ParentHash
            | BlockField::StateRoot
            | BlockField::TransactionsRoot
            | BlockField::ReceiptsRoot
            | BlockField::BaseFeePerGas
            | BlockField::Chain
    )
}

/// Returns true if a BlockId is a concrete number (not a tag like latest/earliest/pending).
fn block_id_is_concrete(id: &BlockId) -> bool {
    match id {
        BlockId::Number(BlockNumberOrTag::Number(_)) => true,
        BlockId::Range(range) => {
            matches!(range.start(), BlockNumberOrTag::Number(_))
                && range
                    .end()
                    .map_or(true, |e| matches!(e, BlockNumberOrTag::Number(_)))
        }
        _ => false,
    }
}

/// Determines if a block query for a given chain should use the Portal.
fn should_use_portal(chain: &ChainOrRpc, fields: &[BlockField], ids: &[BlockId]) -> bool {
    let dataset = match chain {
        ChainOrRpc::Chain(c) => c.portal_dataset(),
        ChainOrRpc::Rpc(_) => None,
    };

    dataset.is_some()
        && fields.iter().all(|f| field_supported_by_portal(f))
        && ids.iter().all(block_id_is_concrete)
}

pub async fn resolve_block_query(
    block: &Block,
    chains: &[ChainOrRpc],
) -> Result<Vec<BlockQueryRes>> {
    let ids = match block.ids() {
        Some(ids) => ids,
        None => return Err(BlockResolverErrors::IdsNotSet.into()),
    };

    // Validate block ranges before routing to Portal or RPC
    for id in ids {
        if let BlockId::Range(range) = id {
            if let (BlockNumberOrTag::Number(start), Some(BlockNumberOrTag::Number(end))) =
                (range.start(), range.end())
            {
                if start > end {
                    return Err(
                        crate::common::block::BlockRangeError::StartBlockMustBeLessThanEndBlock
                            .into(),
                    );
                }
            }
        }
    }

    let mut all_results = Vec::new();

    for chain in chains {
        let results = if should_use_portal(chain, block.fields(), ids) {
            resolve_blocks_via_portal(block, chain).await?
        } else {
            resolve_blocks_via_rpc(block, chain).await?
        };
        all_results.extend(results);
    }

    Ok(all_results)
}

// ---------------------------------------------------------------------------
// Portal path
// ---------------------------------------------------------------------------

async fn resolve_blocks_via_portal(
    block: &Block,
    chain: &ChainOrRpc,
) -> Result<Vec<BlockQueryRes>> {
    let chain_enum = match chain {
        ChainOrRpc::Chain(c) => c.clone(),
        _ => unreachable!("should_use_portal guards against Rpc variant"),
    };
    let dataset = chain_enum.portal_dataset().unwrap();
    let fields = block.fields();
    let ids = block.ids().unwrap();

    let mut all_results = Vec::new();

    for id in ids {
        let (from_block, to_block) = block_id_to_range(id);

        // Build field selection for Portal
        let mut block_fields = serde_json::Map::new();
        // Always request number so we can identify blocks
        block_fields.insert("number".into(), json!(true));
        for field in fields {
            if let Some(portal_name) = block_field_to_portal_name(field) {
                block_fields.insert(portal_name.into(), json!(true));
            }
        }

        let query = json!({
            "type": "evm",
            "fromBlock": from_block,
            "toBlock": to_block,
            "fields": {
                "block": block_fields
            }
        });

        let response = portal_query(dataset, &query).await?;

        for portal_block in &response {
            let header = match portal_block.get("header") {
                Some(h) => h,
                None => continue,
            };

            let result = parse_portal_block_header(header, fields, &chain_enum);
            all_results.push(result);
        }
    }

    Ok(all_results)
}

/// Converts a BlockId to a (fromBlock, toBlock) range of concrete u64 numbers.
fn block_id_to_range(id: &BlockId) -> (u64, u64) {
    match id {
        BlockId::Number(BlockNumberOrTag::Number(n)) => (*n, *n),
        BlockId::Range(range) => {
            let start = match range.start() {
                BlockNumberOrTag::Number(n) => n,
                _ => unreachable!("block_id_is_concrete guards this"),
            };
            let end = range.end().map_or(start, |e| match e {
                BlockNumberOrTag::Number(n) => n,
                _ => unreachable!("block_id_is_concrete guards this"),
            });
            (start, end)
        }
        _ => unreachable!("block_id_is_concrete guards this"),
    }
}

/// Maps an EQL BlockField to the Portal JSON field name.
fn block_field_to_portal_name(field: &BlockField) -> Option<&'static str> {
    match field {
        BlockField::Number => Some("number"),
        BlockField::Timestamp => Some("timestamp"),
        BlockField::Hash => Some("hash"),
        BlockField::ParentHash => Some("parentHash"),
        BlockField::StateRoot => Some("stateRoot"),
        BlockField::TransactionsRoot => Some("transactionsRoot"),
        BlockField::ReceiptsRoot => Some("receiptsRoot"),
        BlockField::BaseFeePerGas => Some("baseFeePerGas"),
        _ => None,
    }
}

/// Parse a Portal block header JSON into a BlockQueryRes.
fn parse_portal_block_header(
    header: &serde_json::Value,
    fields: &[BlockField],
    chain: &Chain,
) -> BlockQueryRes {
    let mut result = BlockQueryRes::default();

    for field in fields {
        match field {
            BlockField::Number => {
                result.number = header.get("number").and_then(value_to_u64);
            }
            BlockField::Timestamp => {
                result.timestamp = header.get("timestamp").and_then(value_to_u64);
            }
            BlockField::Hash => {
                result.hash = header.get("hash").and_then(value_to_b256);
            }
            BlockField::ParentHash => {
                result.parent_hash = header.get("parentHash").and_then(value_to_b256);
            }
            BlockField::StateRoot => {
                result.state_root = header.get("stateRoot").and_then(value_to_b256);
            }
            BlockField::TransactionsRoot => {
                result.transactions_root =
                    header.get("transactionsRoot").and_then(value_to_b256);
            }
            BlockField::ReceiptsRoot => {
                result.receipts_root = header.get("receiptsRoot").and_then(value_to_b256);
            }
            BlockField::BaseFeePerGas => {
                result.base_fee_per_gas = header.get("baseFeePerGas").and_then(value_to_u64);
            }
            BlockField::Chain => {
                result.chain = Some(chain.clone());
            }
            _ => {} // Non-Portal fields — should not be reached due to should_use_portal guard
        }
    }

    result
}

// ---------------------------------------------------------------------------
// RPC path (original logic, extracted)
// ---------------------------------------------------------------------------

async fn resolve_blocks_via_rpc(
    block: &Block,
    chain: &ChainOrRpc,
) -> Result<Vec<BlockQueryRes>> {
    let fields = block.fields().clone();
    let ids = block.ids().unwrap();

    let provider = Arc::new(ProviderBuilder::new().on_http(chain.rpc_url()?));
    let chain_enum = chain.to_chain().await?;
    let mut all_block_futures = Vec::new();

    for id in ids {
        let provider_clone = provider.clone();
        let chain_clone = chain_enum.clone();
        let fields = fields.clone();

        let block_id = resolve_block_id(id, provider_clone.clone()).await?;
        let block_future = async move {
            get_filtered_blocks(block_id, fields, &provider_clone, &chain_clone).await
        };
        all_block_futures.push(block_future);
    }

    let chain_blocks = try_join_all(all_block_futures).await?;
    Ok(chain_blocks.concat())
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

async fn get_filtered_blocks(
    block_numbers: Vec<u64>,
    fields: Vec<BlockField>,
    provider: &Arc<RootProvider<Http<Client>>>,
    chain: &Chain,
) -> Result<Vec<BlockQueryRes>> {
    let blocks = batch_get_blocks(block_numbers, &provider, false).await?;
    Ok(blocks
        .into_iter()
        .map(|block| filter_rpc_fields(block, &fields, &chain))
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

fn filter_rpc_fields(block: RpcBlock, fields: &[BlockField], chain: &Chain) -> BlockQueryRes {
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
