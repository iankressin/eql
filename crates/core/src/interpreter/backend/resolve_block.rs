use super::resolve_portal::{
    block_id_is_portal_eligible, portal_query, portal_query_with_base_url, resolve_block_id_range,
    value_to_b256, value_to_bloom, value_to_bytes, value_to_u256, value_to_u64,
};
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

/// Determines if a block query for a given chain should use the Portal.
fn should_use_portal(chain: &ChainOrRpc, ids: &[BlockId]) -> bool {
    let dataset = match chain {
        ChainOrRpc::Chain(c) => c.portal_dataset(),
        ChainOrRpc::Rpc(_) => None,
    };
    dataset.is_some() && ids.iter().all(block_id_is_portal_eligible)
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
        let results = if should_use_portal(chain, ids) {
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
    resolve_blocks_via_portal_with_base_url(block, chain, None).await
}

async fn resolve_blocks_via_portal_with_base_url(
    block: &Block,
    chain: &ChainOrRpc,
    base_url: Option<&str>,
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
        let (from_block, to_block) = resolve_block_id_range(dataset, id).await?;

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
            "includeAllBlocks": true,
            "fields": {
                "block": block_fields
            }
        });

        let response = match base_url {
            Some(base_url) => portal_query_with_base_url(base_url, dataset, &query).await?,
            None => portal_query(dataset, &query).await?,
        };

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
        BlockField::Size => Some("size"),
        BlockField::LogsBloom => Some("logsBloom"),
        BlockField::ExtraData => Some("extraData"),
        BlockField::MixHash => Some("mixHash"),
        BlockField::TotalDifficulty => Some("totalDifficulty"),
        BlockField::WithdrawalsRoot => Some("withdrawalsRoot"),
        BlockField::BlobGasUsed => Some("blobGasUsed"),
        BlockField::ExcessBlobGas => Some("excessBlobGas"),
        BlockField::ParentBeaconBlockRoot => Some("parentBeaconBlockRoot"),
        BlockField::Chain => None,
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
                result.transactions_root = header.get("transactionsRoot").and_then(value_to_b256);
            }
            BlockField::ReceiptsRoot => {
                result.receipts_root = header.get("receiptsRoot").and_then(value_to_b256);
            }
            BlockField::BaseFeePerGas => {
                result.base_fee_per_gas = header.get("baseFeePerGas").and_then(value_to_u64);
            }
            BlockField::Size => {
                result.size = header.get("size").and_then(value_to_u256);
            }
            BlockField::LogsBloom => {
                result.logs_bloom = header.get("logsBloom").and_then(value_to_bloom);
            }
            BlockField::ExtraData => {
                result.extra_data = header.get("extraData").and_then(value_to_bytes);
            }
            BlockField::MixHash => {
                result.mix_hash = header.get("mixHash").and_then(value_to_b256);
            }
            BlockField::TotalDifficulty => {
                result.total_difficulty = header.get("totalDifficulty").and_then(value_to_u256);
            }
            BlockField::WithdrawalsRoot => {
                result.withdrawals_root = header.get("withdrawalsRoot").and_then(value_to_b256);
            }
            BlockField::BlobGasUsed => {
                result.blob_gas_used = header.get("blobGasUsed").and_then(value_to_u64);
            }
            BlockField::ExcessBlobGas => {
                result.excess_blob_gas = header.get("excessBlobGas").and_then(value_to_u64);
            }
            BlockField::ParentBeaconBlockRoot => {
                result.parent_beacon_block_root =
                    header.get("parentBeaconBlockRoot").and_then(value_to_b256);
            }
            BlockField::Chain => {
                result.chain = Some(chain.clone());
            }
        }
    }

    result
}

// ---------------------------------------------------------------------------
// RPC path (original logic, extracted)
// ---------------------------------------------------------------------------

async fn resolve_blocks_via_rpc(block: &Block, chain: &ChainOrRpc) -> Result<Vec<BlockQueryRes>> {
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

    #[test]
    fn test_parse_portal_block_header_decodes_all_fields() {
        use serde_json::json;
        let header = json!({
            "number": 1,
            "timestamp": 1438269988u64,
            "hash": "0x88e96d4537bea4d9c05d12549907b32561d3bf31f45aae734cdc119f13406cb6",
            "size": 537,
            "totalDifficulty": "0x7ff800000",
            "baseFeePerGas": null
        });
        let fields = vec![
            BlockField::Number,
            BlockField::Size,
            BlockField::TotalDifficulty,
            BlockField::BaseFeePerGas,
        ];
        let res = parse_portal_block_header(&header, &fields, &Chain::Ethereum);
        assert_eq!(res.number, Some(1));
        assert_eq!(res.size, Some(alloy::primitives::U256::from(537)));
        assert_eq!(
            res.total_difficulty,
            Some(alloy::primitives::U256::from(34351349760u64))
        );
        assert_eq!(res.base_fee_per_gas, None);
    }

    #[test]
    fn test_block_field_mapping_is_exhaustive() {
        // all_variants() returns &'static [BlockField], so `field` is already &BlockField.
        for field in BlockField::all_variants() {
            let mapped = block_field_to_portal_name(field).is_some();
            let local = matches!(field, BlockField::Chain);
            assert!(
                mapped || local,
                "BlockField {:?} not Portal-serviceable",
                field
            );
        }
    }

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

    #[tokio::test]
    async fn test_portal_block_range_requests_and_returns_every_block() {
        let block = Block::new(
            Some(vec![BlockId::Range(BlockRange::new(
                BlockNumberOrTag::Number(50),
                Some(BlockNumberOrTag::Number(52)),
            ))]),
            None,
            vec![BlockField::Number],
        );
        let (base_url, requests, handle) =
            super::super::resolve_portal::test_support::spawn_mock_portal(vec![concat!(
                "{\"header\":{\"number\":\"0x32\"}}\n",
                "{\"header\":{\"number\":\"0x33\"}}\n",
                "{\"header\":{\"number\":\"0x34\"}}\n"
            )
            .to_string()]);

        let results = resolve_blocks_via_portal_with_base_url(
            &block,
            &ChainOrRpc::Chain(Chain::Ethereum),
            Some(&base_url),
        )
        .await
        .unwrap();
        handle.join().expect("mock Portal thread");

        assert_eq!(
            results
                .iter()
                .map(|result| result.number.unwrap())
                .collect::<Vec<_>>(),
            vec![50, 51, 52]
        );

        let requests = requests.lock().expect("captured requests");
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0]["fromBlock"], json!(50));
        assert_eq!(requests[0]["toBlock"], json!(52));
        assert_eq!(requests[0]["includeAllBlocks"], json!(true));
    }
}
