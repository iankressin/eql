use super::resolve_block::{batch_get_blocks, get_block};
use crate::common::{
    block::BlockId,
    chain::ChainOrRpc,
    query_result::TransactionQueryRes,
    transaction::{Transaction, TransactionField},
};
use alloy::{
    consensus::Transaction as ConsensusTransaction,
    primitives::FixedBytes,
    providers::{Provider, ProviderBuilder, RootProvider},
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

/// Resolve the query to get transactions after receiving an transaction entity expression
/// Iterate through entity_ids and map them to a futures list. Execute all futures concurrently and collect the results.
/// The sequence of steps to fetch transactions is:
/// 1. Check if ids are provided.
/// 2. If ids are provided, fetch the transactions.
/// 3. If ids are not provided, fetch the transactions by block number.
/// 4. If ids are not provided, then block number or block range filter must be provided.
/// 5. Fetch the transactions by block number or block range.
/// 6. If both ids and block number or block range filter are provided, then fetch the transactions by ids first, and filter the result by block number or block range.
pub async fn resolve_transaction_query(
    transaction: &Transaction,
    chains: &[ChainOrRpc],
) -> Result<Vec<TransactionQueryRes>> {
    if !transaction.ids().is_some() && !transaction.has_block_filter() {
        return Err(TransactionResolverErrors::MissingTransactionHashOrFilter.into());
    }

    let mut all_results = Vec::new();

    for chain in chains {
        let provider = Arc::new(ProviderBuilder::new().on_http(chain.rpc_url()?));

        // Fetch transactions for this chain
        let rpc_transactions = match transaction.ids() {
            Some(ids) => get_transactions_by_ids(ids, &provider).await?,
            None => {
                let block_id = transaction.get_block_id_filter()?;
                get_transactions_by_block_id(block_id, &provider).await?
            }
        };

        let result_futures = rpc_transactions
            .iter()
            .map(|t| pick_transaction_fields(t, transaction.fields(), &provider, chain));
        let tx_res = try_join_all(result_futures).await?;

        // Filter and collect results for this chain
        let filtered_tx_res: Vec<TransactionQueryRes> = tx_res
            .into_iter()
            .filter(|t| t.has_value() && transaction.filter(t))
            .collect();

        all_results.extend(filtered_tx_res);
    }

    Ok(all_results)
}

async fn get_transactions_by_ids(
    ids: &Vec<FixedBytes<32>>,
    provider: &RootProvider<Http<Client>>,
) -> Result<Vec<RpcTransaction>> {
    let mut tx_futures = Vec::new();
    for id in ids {
        let provider = provider.clone();
        // let tx = provider
        //     .raw_request("eth_getTransactionByHash".into(), (*id,))
        //     .await?;
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
    chain: &ChainOrRpc,
) -> Result<TransactionQueryRes> {
    let mut result = TransactionQueryRes::default();
    let chain = chain.to_chain().await?;

    for field in fields {
        match field {
            TransactionField::Type => {
                result.r#type = Some(tx.inner.tx_type().into());
            }
            TransactionField::AuthorizationList => {
                result.authorization_list = tx.inner.authorization_list().map(|a| a.to_vec());
            }
            TransactionField::Hash => {
                result.hash = Some(tx.inner.tx_hash().clone());
            }
            TransactionField::From => {
                result.from = Some(tx.from);
            }
            TransactionField::To => {
                result.to = tx.inner.to().clone();
            }
            TransactionField::Data => {
                result.data = Some(tx.inner.input().clone());
            }
            TransactionField::Value => {
                result.value = Some(tx.inner.value().clone());
            }
            TransactionField::GasPrice => {
                result.gas_price = tx.inner.gas_price();
            }
            TransactionField::EffectiveGasPrice => {
                result.effective_gas_price = tx.effective_gas_price;
            }
            TransactionField::GasLimit => {
                result.gas_limit = Some(tx.inner.gas_limit());
            }
            TransactionField::Status => {
                match provider
                    .get_transaction_receipt(tx.inner.tx_hash().clone())
                    .await?
                {
                    Some(receipt) => {
                        result.status = Some(receipt.status());
                    }
                    None => {
                        result.status = None;
                    }
                }
            }
            TransactionField::ChainId => {
                result.chain_id = tx.inner.chain_id();
            }
            TransactionField::V => {
                result.v = Some(tx.inner.signature().v());
            }
            TransactionField::R => {
                result.r = Some(tx.inner.signature().r());
            }
            TransactionField::S => {
                result.s = Some(tx.inner.signature().s());
            }
            TransactionField::MaxFeePerBlobGas => {
                result.max_fee_per_blob_gas = tx.inner.max_fee_per_blob_gas();
            }
            TransactionField::MaxFeePerGas => {
                result.max_fee_per_gas = Some(tx.inner.max_fee_per_gas());
            }
            TransactionField::MaxPriorityFeePerGas => {
                result.max_priority_fee_per_gas = tx.inner.max_priority_fee_per_gas();
            }
            TransactionField::YParity => {
                result.y_parity = Some(tx.inner.signature().v());
            }
            TransactionField::Chain => {
                result.chain = Some(chain.clone());
            }
        }
    }

    Ok(result)
}
