use std::error::Error;

use crate::common::{query_result::BlockQueryRes, types::BlockField};
use alloy::{
    eips::BlockNumberOrTag,
    providers::{Provider, RootProvider},
    transports::http::{Client, Http},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, thiserror::Error)]
pub enum BlockResolverErrors {
    #[error("Unable to fetch block number for tag {0}")]
    UnableToFetchBlockNumber(BlockNumberOrTag),
    #[error("Start block must be greater than end block")]
    StartBlockMustBeGreaterThanEndBlock,
}

/// Block resolver is responsible for receiving a get expression
/// and resolving it to a [`BlockQueryRes`].
pub async fn resolve_block_query(
    start_block: BlockNumberOrTag,
    end_block: Option<BlockNumberOrTag>,
    fields: Vec<BlockField>,
    provider: &RootProvider<Http<Client>>,
) -> Result<Vec<BlockQueryRes>, Box<dyn Error>> {
    let start_block_number = get_block_number_from_tag(&provider, start_block).await?;
    let end_block_number = match end_block {
        Some(end) => Some(get_block_number_from_tag(&provider, end).await?),
        _ => None,
    };

    // This check is being done here, because it's the first time that we have the block numbers
    if let Some(end) = end_block_number {
        if start_block_number > end {
            return Err(BlockResolverErrors::StartBlockMustBeGreaterThanEndBlock.into());
        }
    }

    match end_block_number {
        Some(number) => batch_get_block(start_block_number, number, fields, &provider).await,
        None => {
            let block_res = get_block(start_block, fields, &provider).await?;

            Ok(vec![block_res])
        }
    }
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
        // TODO: handle error
        None => panic!("Block not found"),
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
    use alloy::providers::ProviderBuilder;

    use crate::common::chain::Chain;

    use super::resolve_block_query;

    #[tokio::test]
    async fn test_resolve_block_query_when_start_is_greater_than_end() {
        let start_block = 10;
        let end_block = 5;
        let fields = vec![];
        let provider = ProviderBuilder::new().on_http(Chain::Sepolia.rpc_url().parse().unwrap());

        let result = resolve_block_query(
            start_block.into(),
            Some(end_block.into()),
            fields,
            &provider,
        )
        .await
        .unwrap_err()
        .to_string();

        assert_eq!(result, "Start block must be greater than end block");
    }
}
