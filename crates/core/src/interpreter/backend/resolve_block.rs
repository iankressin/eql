use std::error::Error;

use crate::common::{entity_id::EntityId, query_result::BlockQueryRes, types::BlockField};
use alloy::{
    eips::BlockNumberOrTag,
    providers::{Provider, RootProvider},
    transports::http::{Client, Http},
};
use serde::{Deserialize, Serialize};
use futures::future::try_join_all;

#[derive(Debug, Serialize, Deserialize, thiserror::Error)]
pub enum BlockResolverErrors {
    #[error("Unable to fetch block number for tag {0}")]
    UnableToFetchBlockNumber(BlockNumberOrTag),
    #[error("Start block must be greater than end block")]
    StartBlockMustBeGreaterThanEndBlock,
    #[error("Mismatch between Entity and EntityId")]
    MismatchEntityAndEntityId,
}

pub async fn resolve_block_query(
    entity_ids: Vec<EntityId>, 
    fields: Vec<BlockField>,
    provider: &RootProvider<Http<Client>>,
) -> Result<Vec<BlockQueryRes>, Box<dyn Error>> {
    // Create a vector to store individual futures.
    let mut block_futures = Vec::new();

    // Iterate through entity_ids and map them to futures.
    for entity_id in entity_ids {
        let fields = fields.clone(); // Clone fields for each async block.
        let provider = provider.clone(); // Clone the provider if necessary, ensure it's Send + Sync.

        let block_future = async move {
            match entity_id {
                EntityId::Block(block_range) => {
                    // Extract start and end blocks from the block range.
                    let (start_block, end_block) = block_range.range();
                    let start_block_number = get_block_number_from_tag(&provider, start_block).await?;

                    // Get the end block number, if specified.
                    let end_block_number = match end_block {
                        Some(end) => Some(get_block_number_from_tag(&provider, end).await?),
                        None => None,
                    };

                    // Ensure start block is not greater than the end block.
                    if let Some(end_number) = end_block_number {
                        if start_block_number > end_number {
                            return Err(BlockResolverErrors::StartBlockMustBeGreaterThanEndBlock.into());
                        }
                        // Fetch blocks within the specified range.
                        batch_get_block(start_block_number, end_number, fields, &provider).await
                    } else {
                        // Fetch a single block if no end block is provided.
                        batch_get_block(start_block_number, start_block_number, fields, &provider).await
                    }
                }
                // Ensure all entity IDs are of the variant EntityId::Block.
                _ => Err(Box::new(BlockResolverErrors::MismatchEntityAndEntityId).into()),
            }
        };

        // Add the future to the list.
        block_futures.push(block_future);
    }

    // Execute all futures concurrently and collect the results.
    let block_res: Vec<Vec<BlockQueryRes>> = try_join_all(block_futures).await?;

    // Flatten the Vec<Vec<BlockQueryRes>> into a single Vec<BlockQueryRes>.
   Ok(block_res.into_iter().flatten().collect())
}

async fn batch_get_block(
    start_block: u64,
    end_block: u64,
    fields: Vec<BlockField>,
    provider: &RootProvider<Http<Client>>,
) -> Result<Vec<BlockQueryRes>, Box<dyn Error>> {
    let mut result: Vec<BlockQueryRes> = vec![];

    for block_number in start_block..=end_block {
        let block = get_block(
            BlockNumberOrTag::Number(block_number),
            fields.clone(),
            &provider,
        )
        .await?;
        result.push(block);
    }

    Ok(result)
}

async fn get_block(
    block_id: BlockNumberOrTag,
    fields: Vec<BlockField>,
    provider: &RootProvider<Http<Client>>,
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
    provider: &RootProvider<Http<Client>>,
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
    use alloy::providers::ProviderBuilder;
    use crate::common::{chain::Chain, entity_filter::BlockRange};

    #[tokio::test]
    async fn test_resolve_block_query_when_start_is_greater_than_end() {
        // Define start and end blocks where start is greater than end
        let start_block = 10;
        let end_block = 5;
        let fields = vec![]; // Empty fields for simplicity
        let provider = ProviderBuilder::new().on_http(Chain::Sepolia.rpc_url().parse().unwrap());

        // Create an entity_id with a block range where start > end
        let entity_id = EntityId::Block(BlockRange::new(start_block.into(), Some(end_block.into())));

        // Call resolve_block_query and handle the error
        let result = resolve_block_query(
            vec![entity_id], // Wrap the entity_id in a vector as expected by the function
            fields,
            &provider,
        )
        .await
        .unwrap_err()
        .to_string();

        // Assert that the error message is as expected
        assert_eq!(result, "Start block must be greater than end block");
    }
}
