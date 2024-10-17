use crate::common::{
    query_result::TransactionQueryRes,
    transaction::{Transaction, TransactionField},
};
use alloy::{
    primitives::FixedBytes,
    providers::{Provider, RootProvider},
    transports::http::{Client, Http},
};
use futures::future::try_join_all;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize, thiserror::Error)]
pub enum TransactionResolverErrors {
    #[error("Mismatch between Entity and EntityId, {0} can't be resolved as a transaction id")]
    MismatchEntityAndEntityId(String),
}

/// TODO: Handle filter
/// Resolve the query to get transactions after receiving an transaction entity expression
/// Iterate through entity_ids and map them to a futures list. Execute all futures concurrently and collect the results.
pub async fn resolve_transaction_query(
    transaction: &Transaction,
    provider: Arc<RootProvider<Http<Client>>>,
) -> Result<Vec<TransactionQueryRes>, Box<dyn Error>> {
    let mut tx_futures = Vec::new();

    for tx_id in transaction.ids().unwrap() {
        let fields = transaction.fields().clone();
        let provider = provider.clone();
        let tx_future = async move { get_transaction(*tx_id, fields, &provider).await };

        tx_futures.push(tx_future);
    }

    let tx_res = try_join_all(tx_futures).await?;
    Ok(tx_res)
}

async fn get_transaction(
    hash: FixedBytes<32>,
    fields: Vec<TransactionField>,
    provider: &RootProvider<Http<Client>>,
) -> Result<TransactionQueryRes, Box<dyn Error>> {
    let mut result = TransactionQueryRes::default();
    match provider.get_transaction_by_hash(hash).await? {
        Some(tx) => {
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
                    TransactionField::Status => {
                        match provider.get_transaction_receipt(hash).await? {
                            Some(receipt) => {
                                result.status = Some(receipt.status());
                            }
                            None => {
                                result.status = None;
                            }
                        }
                    }
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
        }
        None => panic!("Transaction not found"),
    }

    Ok(result)
}
