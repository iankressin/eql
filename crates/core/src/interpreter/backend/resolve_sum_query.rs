use crate::common::{
    account::{Account, AccountField},
    chain::{Chain, ChainOrRpc},
    ens::NameOrAddress,
    query_result::{ExpressionResult, SumQueryRes},
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
pub fn resolve_sum_query(exp: &ExpressionResult) -> Result<ExpressionResult> {
    let res = ExpressionResult::Sum(vec![SumQueryRes::default()]);
    Ok(res)
}
