use crate::interpreter::frontend::parser::Rule;
use crate::common::{account::Account, block::Block, transaction::Transaction, logs::Logs};
use pest::iterators::Pairs;
use std::error::Error;

#[derive(thiserror::Error, Debug)]
pub enum EntityError {
    #[error("Unexpected token {0}")]
    UnexpectedToken(String),
    #[error("Missing entity")]
    MissingEntity,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Entity {
    Account(Account),
    Block(Block),
    Transaction(Transaction),
    Logs(Logs),
}

impl TryFrom<Pairs<'_, Rule>> for Entity {
    type Error = Box<dyn Error>;

    fn try_from(pairs: Pairs<'_, Rule>) -> Result<Self, Self::Error> {
        for pair in pairs {
            match pair.as_rule() {
                Rule::account_get => {
                    let account = Account::try_from(pair.into_inner())?;
                    return Ok(Entity::Account(account));
                }
                Rule::block_get => {
                    let block = Block::try_from(pair.into_inner())?;
                    return Ok(Entity::Block(block));
                }
                Rule::tx_get => {
                    let tx = Transaction::try_from(pair.into_inner())?;
                    return Ok(Entity::Transaction(tx));
                }
                Rule::log_get => {
                    let logs = Logs::try_from(pair.into_inner())?;
                    return Ok(Entity::Logs(logs));
                }
                _ => return Err(Box::new(EntityError::UnexpectedToken(pair.as_str().to_string()))),
            }
        }
        Err(Box::new(EntityError::MissingEntity))
    }
}