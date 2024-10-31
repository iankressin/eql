use super::account::AccountError;
use super::logs::LogsError;
use super::transaction::TransactionError;
use crate::common::{
    account::Account, block::Block, block::BlockError, logs::Logs, transaction::Transaction,
};
use crate::interpreter::frontend::parser::Rule;
use pest::iterators::Pairs;

#[derive(thiserror::Error, Debug)]
pub enum EntityError {
    #[error("Unexpected token {0}")]
    UnexpectedToken(String),

    #[error("Missing entity")]
    MissingEntity,

    #[error(transparent)]
    TransactionError(#[from] TransactionError),

    #[error(transparent)]
    LogsError(#[from] LogsError),

    #[error(transparent)]
    BlockError(#[from] BlockError),

    #[error(transparent)]
    AccountError(#[from] AccountError),
}

#[derive(Debug, PartialEq)]
pub enum Entity {
    Account(Account),
    Block(Block),
    Transaction(Transaction),
    Logs(Logs),
}

impl TryFrom<Pairs<'_, Rule>> for Entity {
    type Error = EntityError;

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
                _ => return Err(EntityError::UnexpectedToken(pair.as_str().to_string())),
            }
        }
        Err(EntityError::MissingEntity)
    }
}
