use crate::common::{
    chain::Chain, 
    ens::NameOrAddress, 
    entity_id::EntityId, 
    query_result::AccountQueryRes, 
    types::AccountField
};
use std::error::Error;
use alloy::{
    primitives::Address, 
    providers::{Provider, ProviderBuilder, RootProvider}, 
    transports::http::{Client, Http}
};
use futures::future::try_join_all;
use serde::{Deserialize, Serialize};


#[derive(Debug, Serialize, Deserialize, thiserror::Error)]
pub enum AccountResolverErrors {
    #[error("Mismatch between Entity and EntityId, {0} can't be resolved as a account id")]
    MismatchEntityAndEntityId(String),
    #[error("Unable resolve ENS name")]
    EnsResolution,
}

/// Resolve the query to get accounts after receiving an account entity expression
/// Iterate through entity_ids and map them to a futures list. Execute all futures concurrently and collect the results.
pub async fn resolve_account_query(
    entity_ids: Vec<EntityId>, 
    fields: Vec<AccountField>,
    provider: &RootProvider<Http<Client>>,
) -> Result<Vec<AccountQueryRes>, Box<dyn Error>> {
    let mut account_futures = Vec::new();
    for entity_id in entity_ids {
        let fields = fields.clone();
        let provider = provider.clone();
        let account_future = async move {
            match entity_id {
                EntityId::Account(name_or_address) => { 
                    let address = to_address(name_or_address).await?;
                    get_account(address, fields, &provider).await
                },
                id => Err(Box::new(AccountResolverErrors::MismatchEntityAndEntityId(id.to_string())).into()),
            }
        };

    account_futures.push(account_future);
    }

    let account_res = try_join_all(account_futures).await?;
    Ok(account_res)
}

async fn to_address(name_or_address: NameOrAddress) -> Result<Address, AccountResolverErrors> {
    match &name_or_address {
        NameOrAddress::Address(address) => Ok(*address),
        NameOrAddress::Name(_) => {
            let rpc_url = Chain::Ethereum
                .rpc_url()
                .parse()
                .map_err(|_| AccountResolverErrors::EnsResolution)?;

            let provider = ProviderBuilder::new().on_http(rpc_url);

            let address = name_or_address
                .resolve(&provider)
                .await
                .map_err(|_| AccountResolverErrors::EnsResolution)?;

            Ok(address)
        }
    }
}

async fn get_account(
    address: Address,
    fields: Vec<AccountField>,
    provider: &RootProvider<Http<Client>>,
) -> Result<AccountQueryRes, Box<dyn Error>> {
    let mut account = AccountQueryRes::default();

    for field in &fields {
        match field {
            AccountField::Balance => {
                account.balance = Some(provider.get_balance(address).await?);
            }
            AccountField::Nonce => {
                account.nonce = Some(provider.get_transaction_count(address).await?);
            }
            AccountField::Address => {
                account.address = Some(address);
            }
            AccountField::Code => {
                account.code = Some(provider.get_code_at(address).await?);
            }
        }
    }

    Ok(account)
}