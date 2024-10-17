use super::{
    chain::{Chain, ChainOrRpc},
    dump::Dump,
    entity::Entity,
};
use crate::interpreter::frontend::parser::Rule;
use alloy::transports::http::reqwest::Url;
use pest::iterators::Pairs;
use std::error::Error;

#[derive(Debug, PartialEq, Eq)]
pub enum Expression {
    Get(GetExpression),
}

#[derive(Debug, PartialEq, Eq)]
pub struct GetExpression {
    pub entity: Entity,
    pub chain_or_rpc: ChainOrRpc,
    pub dump: Option<Dump>,
}

impl GetExpression {
    fn new(entity: Entity, chain_or_rpc: ChainOrRpc, dump: Option<Dump>) -> Self {
        Self {
            entity,
            chain_or_rpc,
            dump,
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum GetExpressionError {
    #[error("Unexpected token {0}")]
    UnexpectedToken(String),
    #[error("Missing entity")]
    MissingEntity,
    #[error("Missing chain or RPC")]
    MissingChainOrRpc,
    #[error("Missing query")]
    MissingQuery,
}

impl TryFrom<Pairs<'_, Rule>> for GetExpression {
    type Error = Box<dyn Error>;

    fn try_from(pairs: Pairs<'_, Rule>) -> Result<Self, Self::Error> {
        let mut entity: Option<Entity> = None;
        let mut chain_or_rpc: Option<ChainOrRpc> = None;
        let mut dump: Option<Dump> = None;

        for pair in pairs {
            match pair.as_rule() {
                Rule::entity => {
                    entity = Some(Entity::try_from(pair.into_inner())?);
                }
                Rule::chain => {
                    let chain = Chain::try_from(pair.into_inner())?;
                    chain_or_rpc = Some(ChainOrRpc::Chain(chain));
                }
                Rule::rpc_url => {
                    let url = Url::parse(&pair.as_str().to_string())?;
                    chain_or_rpc = Some(ChainOrRpc::Rpc(url));
                }
                Rule::dump => {
                    dump = Some(Dump::try_from(pair.into_inner())?);
                }
                _ => {
                    return Err(Box::new(GetExpressionError::UnexpectedToken(
                        pair.as_str().to_string(),
                    )))
                }
            }
        }

        // Ensure all required fields are initialized
        let entity = entity.ok_or_else(|| Box::new(GetExpressionError::MissingEntity))?;
        let chain_or_rpc =
            chain_or_rpc.ok_or_else(|| Box::new(GetExpressionError::MissingChainOrRpc))?;

        Ok(GetExpression::new(entity, chain_or_rpc, dump))
    }
}
