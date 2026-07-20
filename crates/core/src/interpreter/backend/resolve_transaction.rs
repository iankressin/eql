use super::resolve_block::{batch_get_blocks, get_block};
use super::resolve_portal::{
    block_id_is_portal_eligible, portal_query, portal_query_with_base_url, resolve_block_id_range,
    value_to_address, value_to_b256, value_to_bytes, value_to_parity_bool, value_to_status_bool,
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
    resolve_transactions_via_portal_with_base_url(transaction, chain, None).await
}

async fn resolve_transactions_via_portal_with_base_url(
    transaction: &Transaction,
    chain: &ChainOrRpc,
    base_url: Option<&str>,
) -> Result<Vec<TransactionQueryRes>> {
    let chain_enum = match chain {
        ChainOrRpc::Chain(c) => c.clone(),
        _ => unreachable!("should_use_portal guards against Rpc variant"),
    };
    let dataset = chain_enum.portal_dataset().unwrap();
    let internal_fields = transaction_internal_fields(transaction);

    let block_id = transaction.get_block_id_filter()?;
    let (from_block, to_block) = resolve_block_id_range(dataset, block_id).await?;

    // Build Portal transaction field selection
    let mut tx_fields = serde_json::Map::new();
    for field in &internal_fields {
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
            "block": { "number": true },
            "transaction": tx_fields
        },
        "transactions": [tx_filter]
    });

    let response = match base_url {
        Some(base_url) => portal_query_with_base_url(base_url, dataset, &query).await?,
        None => portal_query(dataset, &query).await?,
    };

    let mut results = Vec::new();
    for portal_block in &response {
        if let Some(txs) = portal_block.get("transactions").and_then(|t| t.as_array()) {
            for tx in txs {
                let internal_row = parse_portal_transaction(tx, &internal_fields, &chain_enum);
                if let Some(projected_row) =
                    filter_and_project_transaction_row(transaction, &internal_row)
                {
                    results.push(projected_row);
                }
            }
        }
    }

    Ok(results)
}

fn tx_filter_field(filter: &TransactionFilter) -> Option<TransactionField> {
    match filter {
        TransactionFilter::Type(_) => Some(TransactionField::Type),
        TransactionFilter::Hash(_) => Some(TransactionField::Hash),
        TransactionFilter::From(_) => Some(TransactionField::From),
        TransactionFilter::To(_) => Some(TransactionField::To),
        TransactionFilter::Data(_) => Some(TransactionField::Data),
        TransactionFilter::Value(_) => Some(TransactionField::Value),
        TransactionFilter::GasPrice(_) => Some(TransactionField::GasPrice),
        TransactionFilter::GasLimit(_) => Some(TransactionField::GasLimit),
        TransactionFilter::EffectiveGasPrice(_) => Some(TransactionField::EffectiveGasPrice),
        TransactionFilter::ChainId(_) => Some(TransactionField::ChainId),
        TransactionFilter::BlockId(_) => None,
        TransactionFilter::Status(_) => Some(TransactionField::Status),
        TransactionFilter::V(_) => Some(TransactionField::V),
        TransactionFilter::R(_) => Some(TransactionField::R),
        TransactionFilter::S(_) => Some(TransactionField::S),
        TransactionFilter::MaxFeePerBlobGas(_) => Some(TransactionField::MaxFeePerBlobGas),
        TransactionFilter::MaxFeePerGas(_) => Some(TransactionField::MaxFeePerGas),
        TransactionFilter::MaxPriorityFeePerGas(_) => Some(TransactionField::MaxPriorityFeePerGas),
        TransactionFilter::YParity(_) => Some(TransactionField::YParity),
    }
}

fn transaction_internal_fields(transaction: &Transaction) -> Vec<TransactionField> {
    let mut fields = transaction.fields().clone();

    if let Some(filters) = transaction.filters() {
        for filter in filters {
            if let Some(field) = tx_filter_field(filter) {
                if !fields.contains(&field) {
                    fields.push(field);
                }
            }
        }
    }

    // Portal may omit yParity for legacy transactions, so its internal row also needs v
    // in order to derive the parity. Keeping the dependency here makes both routes use the
    // same extraction contract while the final projection still hides v when unrequested.
    if fields.contains(&TransactionField::YParity) && !fields.contains(&TransactionField::V) {
        fields.push(TransactionField::V);
    }

    fields
}

fn project_transaction_row(
    row: &TransactionQueryRes,
    fields: &[TransactionField],
) -> TransactionQueryRes {
    let mut projected = TransactionQueryRes::default();

    for field in fields {
        match field {
            TransactionField::Type => projected.r#type = row.r#type,
            TransactionField::Hash => projected.hash = row.hash,
            TransactionField::From => projected.from = row.from,
            TransactionField::To => projected.to = row.to,
            TransactionField::Data => projected.data = row.data.clone(),
            TransactionField::Value => projected.value = row.value,
            TransactionField::GasPrice => projected.gas_price = row.gas_price,
            TransactionField::GasLimit => projected.gas_limit = row.gas_limit,
            TransactionField::EffectiveGasPrice => {
                projected.effective_gas_price = row.effective_gas_price
            }
            TransactionField::Status => projected.status = row.status,
            TransactionField::ChainId => projected.chain_id = row.chain_id,
            TransactionField::V => projected.v = row.v,
            TransactionField::R => projected.r = row.r,
            TransactionField::S => projected.s = row.s,
            TransactionField::MaxFeePerBlobGas => {
                projected.max_fee_per_blob_gas = row.max_fee_per_blob_gas
            }
            TransactionField::MaxFeePerGas => projected.max_fee_per_gas = row.max_fee_per_gas,
            TransactionField::MaxPriorityFeePerGas => {
                projected.max_priority_fee_per_gas = row.max_priority_fee_per_gas
            }
            TransactionField::YParity => projected.y_parity = row.y_parity,
            TransactionField::Chain => projected.chain = row.chain.clone(),
            TransactionField::AuthorizationList => {
                projected.authorization_list = row.authorization_list.clone()
            }
        }
    }

    projected
}

fn filter_and_project_transaction_row(
    transaction: &Transaction,
    internal_row: &TransactionQueryRes,
) -> Option<TransactionQueryRes> {
    transaction
        .filter(internal_row)
        .then(|| project_transaction_row(internal_row, transaction.fields()))
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
        // Not requested from Portal:
        TransactionField::Chain => None,             // set locally
        TransactionField::AuthorizationList => None, // no Portal field (EIP-7702)
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
            TransactionField::EffectiveGasPrice => {
                result.effective_gas_price = tx.get("effectiveGasPrice").and_then(value_to_u128);
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
                result.y_parity = tx
                    .get("yParity")
                    .and_then(value_to_parity_bool)
                    .or_else(|| tx.get("v").and_then(value_to_parity_bool));
            }
            TransactionField::Chain => {
                result.chain = Some(chain.clone());
            }
            TransactionField::AuthorizationList => {
                // Not available on Portal (EIP-7702); left as None. By-hash queries (RPC) fill it.
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

    let internal_fields = transaction_internal_fields(transaction);
    let result_futures = rpc_transactions
        .iter()
        .map(|t| pick_transaction_fields(t, &internal_fields, &provider, chain));
    let internal_rows = try_join_all(result_futures).await?;

    let filtered_tx_res = internal_rows
        .iter()
        .filter_map(|row| filter_and_project_transaction_row(transaction, row))
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
    fields: &[TransactionField],
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
    use crate::common::{
        block::BlockRange,
        filters::{ComparisonFilter, EqualityFilter, FilterType},
    };
    use alloy::{
        eips::BlockNumberOrTag,
        primitives::{address, U256},
    };

    #[test]
    fn test_parse_portal_transaction_decodes_signature_fields() {
        use serde_json::json;
        let tx = json!({
            "effectiveGasPrice": 10209184711u64,
            "v": "0x0",
            "yParity": "0x0",
            "maxFeePerBlobGas": null
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
        assert_eq!(res.authorization_list, None);
    }

    #[test]
    fn test_tx_field_mapping_is_exhaustive() {
        // all_variants() returns &'static [TransactionField]; `field` is already &TransactionField.
        for field in TransactionField::all_variants() {
            let mapped = tx_field_to_portal_name(field).is_some();
            let local = matches!(
                field,
                TransactionField::Chain | TransactionField::AuthorizationList
            );
            assert!(
                mapped || local,
                "TransactionField {:?} not Portal-serviceable",
                field
            );
        }
    }

    #[tokio::test]
    async fn test_portal_transaction_pagination_uses_internal_fields_without_projecting_them() {
        let sender = address!("1000000000000000000000000000000000000001");
        let transaction = Transaction::new(
            None,
            Some(vec![
                TransactionFilter::BlockId(BlockId::Range(BlockRange::new(
                    BlockNumberOrTag::Number(10),
                    Some(BlockNumberOrTag::Number(11)),
                ))),
                TransactionFilter::From(EqualityFilter::Eq(sender)),
                TransactionFilter::Value(FilterType::Comparison(ComparisonFilter::Gte(
                    U256::from(50),
                ))),
            ]),
            vec![TransactionField::AuthorizationList],
        );
        let (base_url, requests, handle) =
            super::super::resolve_portal::test_support::spawn_mock_portal(vec![
                concat!(
                    "{\"header\":{\"number\":\"0xa\"},\"transactions\":[{",
                    "\"hash\":\"0x0000000000000000000000000000000000000000000000000000000000000001\",",
                    "\"from\":\"0x1000000000000000000000000000000000000001\",",
                    "\"value\":\"0x64\"}]}\n"
                )
                .to_string(),
                concat!(
                    "{\"header\":{\"number\":\"0xb\"},\"transactions\":[{",
                    "\"hash\":\"0x0000000000000000000000000000000000000000000000000000000000000002\",",
                    "\"from\":\"0x1000000000000000000000000000000000000001\",",
                    "\"value\":\"0x65\"}]}\n"
                )
                .to_string(),
            ]);

        let results = resolve_transactions_via_portal_with_base_url(
            &transaction,
            &ChainOrRpc::Chain(Chain::Ethereum),
            Some(&base_url),
        )
        .await
        .unwrap();
        handle.join().expect("mock Portal thread");

        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|result| !result.has_value()));

        let requests = requests.lock().expect("captured requests");
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0]["fromBlock"], json!(10));
        assert_eq!(requests[1]["fromBlock"], json!(11));
        for request in requests.iter() {
            assert_eq!(request["fields"]["block"]["number"], json!(true));
            assert_eq!(request["fields"]["transaction"]["from"], json!(true));
            assert_eq!(request["fields"]["transaction"]["value"], json!(true));
        }
    }

    #[test]
    fn test_rpc_filtering_uses_internal_fields_and_keeps_null_only_projection() {
        let sender = address!("1000000000000000000000000000000000000001");
        let transaction = Transaction::new(
            None,
            Some(vec![TransactionFilter::From(EqualityFilter::Eq(sender))]),
            vec![TransactionField::AuthorizationList],
        );

        assert_eq!(
            transaction_internal_fields(&transaction),
            vec![TransactionField::AuthorizationList, TransactionField::From]
        );

        let internal_row = TransactionQueryRes {
            from: Some(sender),
            authorization_list: None,
            ..TransactionQueryRes::default()
        };
        let projected = filter_and_project_transaction_row(&transaction, &internal_row)
            .expect("the RPC row should pass its unprojected sender filter");

        assert_eq!(
            projected.from, None,
            "filter-only fields must stay internal"
        );
        assert_eq!(projected.authorization_list, None);
        assert!(
            !projected.has_value(),
            "a legacy transaction projected only to authorization_list is a valid null-only row"
        );
    }

    #[tokio::test]
    async fn test_y_parity_projection_derives_legacy_v_without_exposing_v() {
        let transaction = Transaction::new(
            None,
            Some(vec![TransactionFilter::BlockId(BlockId::Range(
                BlockRange::new(BlockNumberOrTag::Number(20), None),
            ))]),
            vec![TransactionField::YParity],
        );
        let (base_url, requests, handle) =
            super::super::resolve_portal::test_support::spawn_mock_portal(vec![concat!(
                "{\"header\":{\"number\":\"0x14\"},\"transactions\":[{",
                "\"hash\":\"0x0000000000000000000000000000000000000000000000000000000000000003\",",
                "\"v\":\"0x1b\",\"yParity\":null}]}\n"
            )
            .to_string()]);

        let results = resolve_transactions_via_portal_with_base_url(
            &transaction,
            &ChainOrRpc::Chain(Chain::Ethereum),
            Some(&base_url),
        )
        .await
        .unwrap();
        handle.join().expect("mock Portal thread");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].y_parity, Some(false));
        assert_eq!(results[0].v, None);

        let requests = requests.lock().expect("captured requests");
        assert_eq!(requests[0]["fields"]["transaction"]["yParity"], json!(true));
        assert_eq!(requests[0]["fields"]["transaction"]["v"], json!(true));
    }

    #[test]
    fn test_should_use_portal_accepts_the_e2e_block_range_shape() {
        // Pins the routing decision the execution_engine Portal e2e tests rely
        // on: their result-shape assertions alone would also pass via RPC.
        let transaction = Transaction::new(
            None,
            Some(vec![TransactionFilter::BlockId(BlockId::Range(
                BlockRange::new(
                    BlockNumberOrTag::Number(20_000_000),
                    Some(BlockNumberOrTag::Number(20_000_000)),
                ),
            ))]),
            vec![TransactionField::Hash],
        );

        assert!(should_use_portal(
            &ChainOrRpc::Chain(Chain::Ethereum),
            &transaction
        ));
    }

    #[test]
    fn test_should_use_portal_rejects_rpc_only_shapes() {
        let range_filter = || {
            TransactionFilter::BlockId(BlockId::Range(BlockRange::new(
                BlockNumberOrTag::Number(1),
                Some(BlockNumberOrTag::Number(2)),
            )))
        };
        let ethereum = ChainOrRpc::Chain(Chain::Ethereum);

        // By-hash queries: Portal has no hash filter.
        let by_hash = Transaction::new(
            Some(vec![FixedBytes::<32>::ZERO]),
            Some(vec![range_filter()]),
            vec![TransactionField::Hash],
        );
        assert!(!should_use_portal(&ethereum, &by_hash));

        // No block filter: Portal needs a range to scan.
        let no_block_filter = Transaction::new(None, None, vec![TransactionField::Hash]);
        assert!(!should_use_portal(&ethereum, &no_block_filter));

        // A pending bound is not Portal-resolvable.
        let pending = Transaction::new(
            None,
            Some(vec![TransactionFilter::BlockId(BlockId::Range(
                BlockRange::new(BlockNumberOrTag::Number(1), Some(BlockNumberOrTag::Pending)),
            ))]),
            vec![TransactionField::Hash],
        );
        assert!(!should_use_portal(&ethereum, &pending));

        // Explicit RPC URLs must never route to Portal.
        let eligible = Transaction::new(
            None,
            Some(vec![range_filter()]),
            vec![TransactionField::Hash],
        );
        assert!(!should_use_portal(
            &ChainOrRpc::Rpc("http://localhost:8545".parse().unwrap()),
            &eligible
        ));
    }
}
