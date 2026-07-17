use super::resolve_block::{batch_get_blocks, get_block};
use super::resolve_portal::{
    portal_query, value_to_address, value_to_b256, value_to_bytes, value_to_status_bool,
    value_to_u128, value_to_u256, value_to_u64, value_to_u8,
};
use crate::common::{
    block::BlockId,
    chain::{Chain, ChainOrRpc},
    query_result::TransactionQueryRes,
    transaction::{Transaction, TransactionField, TransactionFilter},
};
use alloy::{
    consensus::Transaction as ConsensusTransaction,
    eips::BlockNumberOrTag,
    primitives::FixedBytes,
    providers::{Provider, ProviderBuilder, RootProvider},
    rpc::types::{BlockTransactions, Transaction as RpcTransaction},
    transports::http::{Client, Http},
};
use anyhow::{Ok, Result};
use futures::future::try_join_all;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize, thiserror::Error)]
pub enum TransactionResolverErrors {
    #[error("Mismatch between Entity and EntityId, {0} can't be resolved as a transaction id")]
    MismatchEntityAndEntityId(String),
    #[error("Query should either provide tx hash or block number/range filter")]
    MissingTransactionHashOrFilter,
}

/// Returns true if a TransactionField can be served by the SQD Portal.
fn field_supported_by_portal(field: &TransactionField) -> bool {
    matches!(
        field,
        TransactionField::Type
            | TransactionField::Hash
            | TransactionField::From
            | TransactionField::To
            | TransactionField::Data
            | TransactionField::Value
            | TransactionField::GasPrice
            | TransactionField::GasLimit
            | TransactionField::Status
            | TransactionField::ChainId
            | TransactionField::MaxFeePerGas
            | TransactionField::MaxPriorityFeePerGas
            | TransactionField::Chain
    )
}

/// Extract block range as concrete (u64, u64) from a BlockId, or None if it contains tags.
fn block_id_to_concrete_range(block_id: &BlockId) -> Option<(u64, u64)> {
    match block_id {
        BlockId::Number(BlockNumberOrTag::Number(n)) => Some((*n, *n)),
        BlockId::Range(range) => {
            let start = match range.start() {
                BlockNumberOrTag::Number(n) => n,
                _ => return None,
            };
            let end = match range.end() {
                Some(BlockNumberOrTag::Number(n)) => n,
                Some(_) => return None,
                None => start,
            };
            Some((start, end))
        }
        _ => None,
    }
}

/// Extract from/to address filters from TransactionFilters for Portal server-side filtering.
fn extract_address_filters(
    filters: Option<&Vec<TransactionFilter>>,
) -> (Vec<String>, Vec<String>) {
    use crate::common::filters::EqualityFilter;

    let mut from_addrs = Vec::new();
    let mut to_addrs = Vec::new();

    if let Some(filters) = filters {
        for filter in filters {
            match filter {
                TransactionFilter::From(EqualityFilter::Eq(addr)) => {
                    from_addrs.push(format!("{:?}", addr));
                }
                TransactionFilter::To(EqualityFilter::Eq(addr)) => {
                    to_addrs.push(format!("{:?}", addr));
                }
                _ => {}
            }
        }
    }

    (from_addrs, to_addrs)
}

/// Determines if a transaction query for a given chain should use the Portal.
fn should_use_portal(chain: &ChainOrRpc, transaction: &Transaction) -> bool {
    // Must be a named chain with Portal dataset
    let dataset = match chain {
        ChainOrRpc::Chain(c) => c.portal_dataset(),
        ChainOrRpc::Rpc(_) => None,
    };
    if dataset.is_none() {
        return false;
    }

    // Must be a block-range query (not hash-based)
    if transaction.ids().is_some() {
        return false;
    }

    if !transaction.has_block_filter() {
        return false;
    }

    // Block range must be concrete numbers
    let block_id = match transaction.get_block_id_filter() {
        std::result::Result::Ok(id) => id,
        Err(_) => return false,
    };
    if block_id_to_concrete_range(block_id).is_none() {
        return false;
    }

    // All requested fields must be Portal-compatible
    transaction
        .fields()
        .iter()
        .all(|f| field_supported_by_portal(f))
}

pub async fn resolve_transaction_query(
    transaction: &Transaction,
    chains: &[ChainOrRpc],
) -> Result<Vec<TransactionQueryRes>> {
    if !transaction.ids().is_some() && !transaction.has_block_filter() {
        return Err(TransactionResolverErrors::MissingTransactionHashOrFilter.into());
    }

    let mut all_results = Vec::new();

    for chain in chains {
        let results = if should_use_portal(chain, transaction) {
            resolve_transactions_via_portal(transaction, chain).await?
        } else {
            resolve_transactions_via_rpc(transaction, chain).await?
        };
        all_results.extend(results);
    }

    Ok(all_results)
}

// ---------------------------------------------------------------------------
// Portal path
// ---------------------------------------------------------------------------

async fn resolve_transactions_via_portal(
    transaction: &Transaction,
    chain: &ChainOrRpc,
) -> Result<Vec<TransactionQueryRes>> {
    let chain_enum = match chain {
        ChainOrRpc::Chain(c) => c.clone(),
        _ => unreachable!("should_use_portal guards against Rpc variant"),
    };
    let dataset = chain_enum.portal_dataset().unwrap();
    let fields = transaction.fields();

    let block_id = transaction.get_block_id_filter()?;
    let (from_block, to_block) = block_id_to_concrete_range(block_id).unwrap();

    // Build Portal transaction field selection
    let mut tx_fields = serde_json::Map::new();
    for field in fields {
        if let Some(portal_name) = tx_field_to_portal_name(field) {
            tx_fields.insert(portal_name.into(), json!(true));
        }
    }
    // Always include hash for dedup/identification
    tx_fields.insert("hash".into(), json!(true));

    // Build Portal transaction filter with from/to if available
    let mut tx_filter = serde_json::Map::new();
    let (from_addrs, to_addrs) = extract_address_filters(transaction.filters());
    if !from_addrs.is_empty() {
        tx_filter.insert("from".into(), json!(from_addrs));
    }
    if !to_addrs.is_empty() {
        tx_filter.insert("to".into(), json!(to_addrs));
    }

    let query = json!({
        "type": "evm",
        "fromBlock": from_block,
        "toBlock": to_block,
        "fields": {
            "transaction": tx_fields
        },
        "transactions": [tx_filter]
    });

    let response = portal_query(dataset, &query).await?;

    let mut results = Vec::new();
    for portal_block in &response {
        if let Some(txs) = portal_block.get("transactions").and_then(|t| t.as_array()) {
            for tx in txs {
                let tx_res = parse_portal_transaction(tx, fields, &chain_enum);
                if tx_res.has_value() && transaction.filter(&tx_res) {
                    results.push(tx_res);
                }
            }
        }
    }

    Ok(results)
}

/// Maps an EQL TransactionField to the Portal JSON field name.
fn tx_field_to_portal_name(field: &TransactionField) -> Option<&'static str> {
    match field {
        TransactionField::Type => Some("type"),
        TransactionField::Hash => Some("hash"),
        TransactionField::From => Some("from"),
        TransactionField::To => Some("to"),
        TransactionField::Data => Some("input"),
        TransactionField::Value => Some("value"),
        TransactionField::GasPrice => Some("gasPrice"),
        TransactionField::GasLimit => Some("gas"),
        TransactionField::Status => Some("status"),
        TransactionField::ChainId => Some("chainId"),
        TransactionField::MaxFeePerGas => Some("maxFeePerGas"),
        TransactionField::MaxPriorityFeePerGas => Some("maxPriorityFeePerGas"),
        _ => None,
    }
}

/// Parse a Portal transaction JSON into a TransactionQueryRes.
fn parse_portal_transaction(
    tx: &serde_json::Value,
    fields: &[TransactionField],
    chain: &Chain,
) -> TransactionQueryRes {
    let mut result = TransactionQueryRes::default();

    for field in fields {
        match field {
            TransactionField::Type => {
                result.r#type = tx.get("type").and_then(value_to_u8);
            }
            TransactionField::Hash => {
                result.hash = tx.get("hash").and_then(value_to_b256).map(|b| b.into());
            }
            TransactionField::From => {
                result.from = tx.get("from").and_then(value_to_address);
            }
            TransactionField::To => {
                result.to = tx.get("to").and_then(value_to_address);
            }
            TransactionField::Data => {
                result.data = tx.get("input").and_then(value_to_bytes);
            }
            TransactionField::Value => {
                result.value = tx.get("value").and_then(value_to_u256);
            }
            TransactionField::GasPrice => {
                result.gas_price = tx.get("gasPrice").and_then(value_to_u128);
            }
            TransactionField::GasLimit => {
                result.gas_limit = tx.get("gas").and_then(value_to_u64);
            }
            TransactionField::Status => {
                result.status = tx.get("status").and_then(value_to_status_bool);
            }
            TransactionField::ChainId => {
                result.chain_id = tx.get("chainId").and_then(value_to_u64);
            }
            TransactionField::MaxFeePerGas => {
                result.max_fee_per_gas = tx.get("maxFeePerGas").and_then(value_to_u128);
            }
            TransactionField::MaxPriorityFeePerGas => {
                result.max_priority_fee_per_gas =
                    tx.get("maxPriorityFeePerGas").and_then(value_to_u128);
            }
            TransactionField::Chain => {
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

async fn resolve_transactions_via_rpc(
    transaction: &Transaction,
    chain: &ChainOrRpc,
) -> Result<Vec<TransactionQueryRes>> {
    let provider = Arc::new(ProviderBuilder::new().on_http(chain.rpc_url()?));

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

    let filtered_tx_res: Vec<TransactionQueryRes> = tx_res
        .into_iter()
        .filter(|t| t.has_value() && transaction.filter(t))
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
