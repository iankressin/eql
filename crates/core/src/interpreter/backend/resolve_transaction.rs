use crate::common::{
    entity_id::EntityId, 
    query_result::TransactionQueryRes, 
    types::TransactionField
};
use std::error::Error;
use alloy::{
    primitives::FixedBytes, 
    providers::{Provider, RootProvider}, 
    transports::http::{Client, Http}
};
use futures::future::try_join_all;
use serde::{Deserialize, Serialize};


#[derive(Debug, Serialize, Deserialize, thiserror::Error)]
pub enum TransactionResolverErrors {
    // #[error("Invalid address")]
    // InvalidAddress,
    #[error("Mismatch between Entity and EntityId")]
    MismatchEntityAndEntityId,
    #[error("Unable resolve ENS name")]
    EnsResolution,
}

pub async fn resolve_transaction_query(
    entity_ids: Vec<EntityId>, 
    fields: Vec<TransactionField>,
    provider: &RootProvider<Http<Client>>,
) -> Result<Vec<TransactionQueryRes>, Box<dyn Error>> {
    // Create a vector to store individual futures, for each request.
    let mut tx_futures = Vec::new();
    // Iterate through entity_ids and map them to futures.
    for entity_id in entity_ids {
        let fields = fields.clone(); // Clone fields for each async block.
        let provider = provider.clone(); // Clone the provider if necessary, ensure it's Send + Sync.
        let tx_future = async move {
        
            match entity_id {
                EntityId::Transaction(hash) => { 
                    get_transaction(hash, fields, &provider).await
                },
                // Ensure all entity IDs are of the variant EntityId::Transaction
                _ => Err(Box::new(TransactionResolverErrors::MismatchEntityAndEntityId).into()),
            }
        };

    // Add the future to the list.
    tx_futures.push(tx_future);
    }

    // Execute all futures concurrently and collect the results.
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