use super::resolve_block::{batch_get_blocks, get_block};
use crate::common::{
    block::BlockId,
    query_result::TransactionQueryRes,
    transaction::{Transaction, TransactionField, TransactionFilter},
};
use alloy::{
    primitives::FixedBytes,
    providers::{Provider, RootProvider},
    rpc::types::{BlockTransactions, Transaction as RpcTransaction},
    transports::http::{Client, Http},
};
use anyhow::{Ok, Result};
use futures::future::try_join_all;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize, thiserror::Error)]
pub enum TransactionResolverErrors {
    #[error("Mismatch between Entity and EntityId, {0} can't be resolved as a transaction id")]
    MismatchEntityAndEntityId(String),
    #[error("Query should either provide tx hash or block number/range filter")]
    MissingTransactionHashOrFilter,
}

/// TODO: Handle filter
/// Resolve the query to get transactions after receiving an transaction entity expression
/// Iterate through entity_ids and map them to a futures list. Execute all futures concurrently and collect the results.
pub async fn resolve_transaction_query(
    transaction: &Transaction,
    provider: Arc<RootProvider<Http<Client>>>,
) -> Result<Vec<TransactionQueryRes>> {
    // The sequence of steps to fetch transactions is:
    // 1. Check if ids are provided.
    // 2. If ids are provided, fetch the transactions.
    // 3. If ids are not provided, fetch the transactions by block number.
    // 4. If ids are not provided, then block number or block range filter must be provided.
    // 5. Fetch the transactions by block number or block range.
    // 6. If both ids and block number or block range filter are provided, then fetch the transactions by ids first, and filter the result by block number or block range.

    let ids_provided = transaction.ids().is_some();
    let block_filter_provided = match transaction.filters() {
        Some(filters) => filters
            .iter()
            .any(|f| matches!(f, TransactionFilter::BlockId(BlockId::Range(_)))),
        None => false,
    };

    if !ids_provided && !block_filter_provided {
        return Err(TransactionResolverErrors::MissingTransactionHashOrFilter.into());
    }

    // In this step we're only fetching transactions, filtering is done in the next step.
    let rpc_transactions = match transaction.ids() {
        Some(ids) => get_transactions_by_ids(ids, &provider).await?,
        None => {
            let block_id = transaction.get_block_id_filter()?;
            get_transactions_by_block_id(block_id, &provider).await?
        }
    };

    let result_futures = rpc_transactions
        .iter()
        .map(|t| pick_transaction_fields(t, transaction.fields(), &provider));
    let tx_res = try_join_all(result_futures).await?;
    let filtered_tx_res = tx_res
        .into_iter()
        .filter(|t| transaction.filter(t))
        .collect();

    Ok(filtered_tx_res)
}

async fn get_transactions_by_ids(
    ids: &Vec<FixedBytes<32>>,
    provider: &RootProvider<Http<Client>>,
) -> Result<Vec<RpcTransaction>> {
    let mut tx_futures = Vec::new();
    for id in ids {
        let provider = provider.clone();
        let tx_future = async move { provider.get_transaction_by_hash(*id).await };

        tx_futures.push(tx_future);
    }

    let tx_res = try_join_all(tx_futures).await?;

    Ok(tx_res.into_iter().filter_map(|t| t).collect())
}

async fn get_transactions_by_block_id(
    block_id: &BlockId,
    provider: &Arc<RootProvider<Http<Client>>>,
) -> Result<Vec<RpcTransaction>> {
    match block_id {
        BlockId::Number(n) => {
            let block = get_block(n.clone(), provider.clone(), true).await?;
            match &block.transactions {
                BlockTransactions::Full(txs) => Ok(txs.clone()),
                _ => panic!("Block transactions should be full"),
            }
        }
        BlockId::Range(r) => {
            let block_numbers = r.resolve_block_numbers(provider).await?;
            let blocks = batch_get_blocks(block_numbers, provider, true).await?;
            let txs = blocks
                .iter()
                .flat_map(|b| match &b.transactions {
                    BlockTransactions::Full(txs) => txs.clone(),
                    _ => panic!("Block transactions should be full"),
                })
                .collect::<Vec<_>>();

            Ok(txs)
        }
    }
}

async fn pick_transaction_fields(
    tx: &RpcTransaction,
    fields: &Vec<TransactionField>,
    provider: &Arc<RootProvider<Http<Client>>>,
) -> Result<TransactionQueryRes> {
    let mut result = TransactionQueryRes::default();

    for field in fields {
        match field {
            TransactionField::TransactionType => {
                result.transaction_type = tx.transaction_type;
            }
            TransactionField::Hash => {
                result.hash = Some(tx.hash);
            }
            TransactionField::From => {
                result.from = Some(tx.from);
            }
            TransactionField::To => {
                result.to = tx.to;
            }
            TransactionField::Data => {
                result.data = Some(tx.input.clone());
            }
            TransactionField::Value => {
                result.value = Some(tx.value);
            }
            TransactionField::GasPrice => {
                result.gas_price = tx.gas_price;
            }
            TransactionField::Gas => {
                result.gas = Some(tx.gas);
            }
            TransactionField::Status => match provider.get_transaction_receipt(tx.hash).await? {
                Some(receipt) => {
                    result.status = Some(receipt.status());
                }
                None => {
                    result.status = None;
                }
            },
            TransactionField::ChainId => {
                result.chain_id = tx.chain_id;
            }
            TransactionField::V => {
                result.v = tx.signature.map_or(None, |s| Some(s.v));
            }
            TransactionField::R => {
                result.r = tx.signature.map_or(None, |s| Some(s.r));
            }
            TransactionField::S => {
                result.s = tx.signature.map_or(None, |s| Some(s.s));
            }
            TransactionField::MaxFeePerBlobGas => {
                result.max_fee_per_blob_gas = tx.max_fee_per_blob_gas;
            }
            TransactionField::MaxFeePerGas => {
                result.max_fee_per_gas = tx.max_fee_per_gas;
            }
            TransactionField::MaxPriorityFeePerGas => {
                result.max_priority_fee_per_gas = tx.max_priority_fee_per_gas;
            }
            TransactionField::YParity => {
                result.y_parity = tx
                    .signature
                    .map_or(None, |s| s.y_parity)
                    .map_or(None, |y| Some(y.0));
            }
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::{block::BlockRange, chain::Chain};
    use alloy::{eips::BlockNumberOrTag, primitives::U256, providers::ProviderBuilder};

    #[tokio::test]
    async fn test_get_transactions_by_block_range() {
        let rpc = Chain::Ethereum.rpc_url().unwrap();
        let provider = Arc::new(ProviderBuilder::new().on_http(rpc));
        let block_id = BlockId::Range(BlockRange::new(10000000.into(), Some(10000015.into())));
        let transactions = get_transactions_by_block_id(&block_id, &provider)
            .await
            .unwrap();

        assert_eq!(transactions.len(), 2394);
    }

    #[tokio::test]
    async fn test_get_transactions_by_block_number() {
        let rpc = Chain::Ethereum.rpc_url().unwrap();
        let provider = Arc::new(ProviderBuilder::new().on_http(rpc));
        let block_id = BlockId::Number(BlockNumberOrTag::Number(21036202));
        let transactions = get_transactions_by_block_id(&block_id, &provider)
            .await
            .unwrap();

        assert_eq!(transactions.len(), 177);
    }

    #[tokio::test]
    async fn test_resolve_query_using_block_range_filter() {
        let rpc = Chain::Ethereum.rpc_url().unwrap();
        let provider = Arc::new(ProviderBuilder::new().on_http(rpc));
        let block_id = BlockId::Range(BlockRange::new(10000000.into(), Some(10000015.into())));
        let transaction = Transaction::new(
            None,
            Some(vec![TransactionFilter::BlockId(block_id)]),
            TransactionField::all_variants().to_vec(),
        );

        let transactions = resolve_transaction_query(&transaction, provider)
            .await
            .unwrap();

        assert_eq!(transactions.len(), 2394);
    }

    #[tokio::test]
    async fn test_resolve_query_using_all_filters() {
        let rpc = Chain::Ethereum.rpc_url().unwrap();
        let provider = Arc::new(ProviderBuilder::new().on_http(rpc));
        let block_id = BlockId::Range(BlockRange::new(10000004.into(), None));
        let transaction = Transaction::new(
            None,
            Some(vec![
                TransactionFilter::BlockId(block_id),
                // TransactionFilter::Value(U256::from(10000000)),
            ]),
            TransactionField::all_variants().to_vec(),
        );

        let transactions = resolve_transaction_query(&transaction, provider)
            .await
            .unwrap();

        println!("{:#?}", transactions);

        assert_eq!(transactions.len(), 127);
    }
}
