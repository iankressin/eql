use super::resolve_block::{batch_get_blocks, get_block};
use super::resolve_portal::{
    block_id_is_portal_eligible, portal_query, resolve_block_id_range, value_to_address,
    value_to_b256, value_to_bytes, value_to_parity_bool, value_to_status_bool, value_to_u128,
    value_to_u256, value_to_u64, value_to_u8,
};
use crate::common::{
    block::BlockId,
    chain::{Chain, ChainOrRpc},
    query_result::TransactionQueryRes,
    transaction::{Transaction, TransactionField, TransactionFilter},
};
use alloy::{
    consensus::Transaction as ConsensusTransaction,
    primitives::FixedBytes,
    providers::{Provider, ProviderBuilder, RootProvider},
    rpc::types::{BlockTransactions, Transaction as RpcTransaction},
    transports::http::{Client, Http},
};
use alloy_eip7702::{Authorization, SignedAuthorization};
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

/// Extract from/to address filters from TransactionFilters for Portal server-side filtering.
fn extract_address_filters(filters: Option<&Vec<TransactionFilter>>) -> (Vec<String>, Vec<String>) {
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
    let dataset = match chain {
        ChainOrRpc::Chain(c) => c.portal_dataset(),
        ChainOrRpc::Rpc(_) => None,
    };
    if dataset.is_none() {
        return false;
    }
    // Portal has no transaction-by-hash filter.
    if transaction.ids().is_some() {
        return false;
    }
    // Portal needs a block range to scan.
    if !transaction.has_block_filter() {
        return false;
    }
    match transaction.get_block_id_filter() {
        std::result::Result::Ok(id) => block_id_is_portal_eligible(id),
        Err(_) => false,
    }
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
    let (from_block, to_block) = resolve_block_id_range(dataset, block_id).await?;

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
        TransactionField::EffectiveGasPrice => Some("effectiveGasPrice"),
        TransactionField::V => Some("v"),
        TransactionField::R => Some("r"),
        TransactionField::S => Some("s"),
        TransactionField::MaxFeePerBlobGas => Some("maxFeePerBlobGas"),
        TransactionField::YParity => Some("yParity"),
        TransactionField::AuthorizationList => Some("authorizationList"),
        // Not requested from Portal:
        TransactionField::Chain => None, // set locally
    }
}

/// Parse an authorization nonce: JSON int, decimal string ("14"), or 0x-hex string ("0xe").
/// Portal encodes authorization nonces as DECIMAL strings (verified live 2026-07-20);
/// the generic `value_to_u64` treats bare strings as hex and would mis-parse "14" as 20.
fn value_to_auth_nonce(v: &serde_json::Value) -> Option<u64> {
    v.as_u64().or_else(|| {
        v.as_str().and_then(|s| {
            if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
                u64::from_str_radix(hex, 16).ok()
            } else {
                s.parse().ok()
            }
        })
    })
}

/// Decode Portal's `authorizationList` into alloy `SignedAuthorization`s.
/// Wire format: `chainId`/`r`/`s` hex strings, `nonce` decimal string, `yParity` JSON int.
/// Empty array (non-type-4 tx) → `None`, matching RPC semantics.
fn value_to_authorization_list(v: &serde_json::Value) -> Option<Vec<SignedAuthorization>> {
    let entries = v.as_array()?;
    if entries.is_empty() {
        return None;
    }
    let mut auths = Vec::with_capacity(entries.len());
    for entry in entries {
        let authorization = Authorization {
            chain_id: entry.get("chainId").and_then(value_to_u64)?,
            address: entry.get("address").and_then(value_to_address)?,
            nonce: entry.get("nonce").and_then(value_to_auth_nonce)?,
        };
        let y_parity = entry.get("yParity").and_then(value_to_parity_bool)? as u8;
        let r = entry.get("r").and_then(value_to_u256)?;
        let s = entry.get("s").and_then(value_to_u256)?;
        auths.push(SignedAuthorization::new_unchecked(
            authorization,
            y_parity,
            r,
            s,
        ));
    }
    Some(auths)
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
            TransactionField::EffectiveGasPrice => {
                result.effective_gas_price = tx.get("effectiveGasPrice").and_then(value_to_u128);
            }
            TransactionField::V => {
                result.v = tx.get("v").and_then(value_to_parity_bool);
            }
            TransactionField::R => {
                result.r = tx.get("r").and_then(value_to_u256);
            }
            TransactionField::S => {
                result.s = tx.get("s").and_then(value_to_u256);
            }
            TransactionField::MaxFeePerBlobGas => {
                result.max_fee_per_blob_gas = tx.get("maxFeePerBlobGas").and_then(value_to_u128);
            }
            TransactionField::YParity => {
                result.y_parity = tx.get("yParity").and_then(value_to_parity_bool);
            }
            TransactionField::AuthorizationList => {
                result.authorization_list = tx
                    .get("authorizationList")
                    .and_then(value_to_authorization_list);
            }
            TransactionField::Chain => {
                result.chain = Some(chain.clone());
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_portal_transaction_decodes_signature_fields() {
        let tx = json!({
            "effectiveGasPrice": 10209184711u64,
            "v": "0x0",
            "yParity": "0x0",
            "maxFeePerBlobGas": null,
            "authorizationList": []
        });
        let fields = vec![
            TransactionField::EffectiveGasPrice,
            TransactionField::V,
            TransactionField::YParity,
            TransactionField::MaxFeePerBlobGas,
            TransactionField::AuthorizationList,
        ];
        let res = parse_portal_transaction(&tx, &fields, &Chain::Ethereum);
        assert_eq!(res.effective_gas_price, Some(10209184711u128));
        assert_eq!(res.v, Some(false));
        assert_eq!(res.y_parity, Some(false));
        assert_eq!(res.max_fee_per_blob_gas, None);
        // Empty authorizationList (non-type-4 tx) decodes to None, matching RPC semantics.
        assert_eq!(res.authorization_list, None);
    }

    #[test]
    fn test_value_to_authorization_list_decodes_portal_wire_format() {
        // Live-verified wire fixture (2026-07-20, tx 0x56eb…b85b): chainId hex string,
        // nonce DECIMAL string (RPC cross-check: Portal "14" == RPC "0xe"), yParity int, r/s hex.
        let v = json!([{
            "chainId": "0x1",
            "address": "0xe6b97aa1490c93c28a14d86c13c9dc9c950643ed",
            "nonce": "14",
            "yParity": 0,
            "r": "0x175dd0b40b1ce179e1194da0ce6011fb98d1ab4738bbb81c1c842f654c914d07",
            "s": "0x79e9f8ff24c50f9e528d045d04d386a8ede6161fa65ee4b9d07c83bf13b1452f"
        }]);
        let auths = value_to_authorization_list(&v).expect("should decode");
        assert_eq!(auths.len(), 1);
        assert_eq!(auths[0].inner().chain_id, 1);
        assert_eq!(auths[0].inner().nonce, 14); // decimal, NOT 0x14=20
        assert_eq!(
            auths[0].inner().address,
            alloy::primitives::address!("e6b97aa1490c93c28a14d86c13c9dc9c950643ed")
        );
        assert_eq!(auths[0].y_parity(), 0);
        assert_eq!(value_to_authorization_list(&json!([])), None);
        assert_eq!(value_to_authorization_list(&json!(null)), None);
    }

    #[test]
    fn test_tx_field_mapping_is_exhaustive() {
        // all_variants() returns &'static [TransactionField]; `field` is already &TransactionField.
        for field in TransactionField::all_variants() {
            let mapped = tx_field_to_portal_name(field).is_some();
            let local = matches!(field, TransactionField::Chain);
            assert!(
                mapped || local,
                "TransactionField {:?} not Portal-serviceable",
                field
            );
        }
    }
}
