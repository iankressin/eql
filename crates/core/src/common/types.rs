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
    Count(CountExpression),
}

#[derive(Debug, PartialEq)]
pub struct CountExpression {
    pub query: GetExpression,
}

impl CountExpression {
    fn new(query: GetExpression) -> Self {
        Self { query }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ExpressionError {
    #[error(transparent)]
    Count(#[from] CountExpressionError),

    #[error(transparent)]
    Get(#[from] GetExpressionError),
}

#[derive(thiserror::Error, Debug)]
pub enum CountExpressionError {
    #[error("Unexpected token: {0}")]
    UnexpectedToken(String),
//     #[error("Missing entity")]
//     MissingEntity,
//     #[error("Missing chain or RPC")]
//     MissingChainOrRpc,
//     #[error("URL parse error: {0}")]
//     UrlParseError(String),
//     #[error(transparent)]
//     EntityError(#[from] EntityError),
//     #[error(transparent)]
//     ChainError(#[from] ChainError),
//     #[error(transparent)]
//     DumpError(#[from] DumpError),
}

impl TryFrom<Pairs<'_, Rule>> for CountExpression {
    type Error = ExpressionError;

    fn try_from(mut pairs: Pairs<'_, Rule>) -> Result<Self, Self::Error> {
        let pair = pairs.next().ok_or(CountExpressionError::UnexpectedToken("Expected COUNT expression".into()))?;

        if pair.as_rule() != Rule::count {
            return Err(CountExpressionError::UnexpectedToken("Expected COUNT expression".into()).into());
        }

        let mut inner_pairs = pair.into_inner();
        let get_pair = inner_pairs.next().ok_or(CountExpressionError::UnexpectedToken("Expected GET expression inside COUNT".into()))?;

        let get_expression = GetExpression::try_from(get_pair.into_inner()).map_err(ExpressionError::Get)?;

        Ok(CountExpression {  query : get_expression })
    }
}


#[derive(Debug, PartialEq)]
pub struct GetExpression {
    pub entity: Entity,
    pub chains: Vec<ChainOrRpc>,
    pub dump: Option<Dump>,
}

impl GetExpression {
    fn new(entity: Entity, chains: Vec<ChainOrRpc>, dump: Option<Dump>) -> Self {
        Self {
            entity,
            chains,
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
        let mut chains: Option<Vec<ChainOrRpc>> = None;
        let mut dump: Option<Dump> = None;

        for pair in pairs {
            match pair.as_rule() {
                Rule::entity => {
                    entity = Some(Entity::try_from(pair.into_inner())?);
                }
                Rule::chain_selector => {
                    let selector = pair.as_str();
                    chains = Some(Chain::from_selector(selector)?);
                }
                Rule::rpc_url => {
                    let url = Url::parse(pair.as_str())
                        .map_err(|e| GetExpressionError::UrlParseError(e.to_string()))?;
                    chains = Some(vec![ChainOrRpc::Rpc(url)]);
                }
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

        Ok(GetExpression::new(
            entity.ok_or(GetExpressionError::MissingEntity)?,
            chains.ok_or(GetExpressionError::MissingChainOrRpc)?,
            dump,
        ))
    }
}
