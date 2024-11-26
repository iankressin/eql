use crate::common::{
    account::{Account, AccountField},
    chain::{Chain, ChainOrRpc},
    ens::NameOrAddress,
    query_result::AccountQueryRes,
};
use alloy::{
    primitives::Address,
    providers::{Provider, ProviderBuilder, RootProvider},
    transports::http::{Client, Http},
};
use anyhow::Result;
use futures::future::try_join_all;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize, thiserror::Error)]
pub enum AccountResolverErrors {
    #[error("Mismatch between Entity and EntityId, {0} can't be resolved as a account id")]
    MismatchEntityAndEntityId(String),
}

/// Resolve the query to get accounts after receiving an account entity expression
/// Iterate through entity_ids and map them to a futures list. Execute all futures concurrently and collect the results.
pub async fn resolve_account_query(
    account: &Account,
    chains: &[ChainOrRpc],
) -> Result<Vec<AccountQueryRes>> {
    let mut all_account_futures = Vec::new();

    for chain in chains {
        let provider = Arc::new(ProviderBuilder::new().on_http(chain.rpc_url()?));

        // TODO: Handle filter
        // TODO: Remove unwrap
        for account_id in account.ids().unwrap() {
            let fields = account.fields().clone();
            let provider = provider.clone();

            let account_future = async move {
                match account_id {
                    NameOrAddress::Address(address) => {
                        get_account(address, fields, &provider, chain).await
                    }
                    NameOrAddress::Name(name) => {
                        let address = to_address(name).await?;
                        get_account(&address, fields, &provider, chain).await
                    }
                }
            };

            all_account_futures.push(account_future);
        }
    }

    let account_res = try_join_all(all_account_futures).await?;
    Ok(account_res)
}

async fn get_account(
    address: &Address,
    fields: Vec<AccountField>,
    provider: &RootProvider<Http<Client>>,
    chain: &ChainOrRpc,
) -> Result<AccountQueryRes> {
    let mut account = AccountQueryRes::default();
    let chain = chain.to_chain().await?;

    for field in &fields {
        match field {
            AccountField::Balance => {
                account.balance = Some(provider.get_balance(*address).await?);
            }
            AccountField::Nonce => {
                account.nonce = Some(provider.get_transaction_count(*address).await?);
            }
            AccountField::Address => {
                account.address = Some(*address);
            }
            AccountField::Code => {
                account.code = Some(provider.get_code_at(*address).await?);
            }
            AccountField::Chain => {
                account.chain = Some(chain.clone());
            }
        }
    }

    Ok(account)
}

async fn to_address(name: &String) -> Result<Address> {
    let rpc_url = Chain::Ethereum.rpc_url()?;
    let provider = ProviderBuilder::new().on_http(rpc_url);
    let address = NameOrAddress::Name(name.clone()).resolve(&provider).await?;
    Ok(address)
}
