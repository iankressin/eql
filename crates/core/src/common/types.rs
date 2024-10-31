use super::{
    chain::{Chain, ChainError, ChainOrRpc},
    dump::{Dump, DumpError},
    entity::{Entity, EntityError},
};
use crate::interpreter::frontend::parser::Rule;
use alloy::transports::http::reqwest::Url;
use pest::iterators::Pairs;

#[derive(Debug, PartialEq)]
pub enum Expression {
    Get(GetExpression),
}

#[derive(Debug, PartialEq)]
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
    #[error("Unexpected token: {0}")]
    UnexpectedToken(String),

    #[error("Missing entity")]
    MissingEntity,

    #[error("Missing chain or RPC")]
    MissingChainOrRpc,

    #[error("URL parse error: {0}")]
    UrlParseError(String),

    #[error(transparent)]
    EntityError(#[from] EntityError),

    #[error(transparent)]
    ChainError(#[from] ChainError),

    #[error(transparent)]
    DumpError(#[from] DumpError),
}

impl TryFrom<Pairs<'_, Rule>> for GetExpression {
    type Error = GetExpressionError;

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
                Rule::rpc_url => match Url::parse(&pair.as_str().to_string()) {
                    Ok(url) => chain_or_rpc = Some(ChainOrRpc::Rpc(url)),
                    Err(e) => return Err(GetExpressionError::UrlParseError(e.to_string())),
                },
                Rule::dump => {
                    dump = Some(Dump::try_from(pair.into_inner())?);
                }
                _ => {
                    return Err(GetExpressionError::UnexpectedToken(
                        pair.as_str().to_string(),
                    ))
                }
            }
        }

        // Ensure all required fields are initialized
        let entity = entity.ok_or_else(|| GetExpressionError::MissingEntity)?;
        let chain_or_rpc = chain_or_rpc.ok_or_else(|| GetExpressionError::MissingChainOrRpc)?;

        Ok(GetExpression::new(entity, chain_or_rpc, dump))
    }
}
