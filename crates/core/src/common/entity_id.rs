use crate::interpreter::frontend::parser::{ParserError, Rule};

use super::{ens::NameOrAddress, entity_filter::BlockRange};
use alloy::{
    eips::BlockNumberOrTag,
    primitives::{Address, FixedBytes},
};
use std::{error::Error, fmt::{self, Display, Formatter}, str::FromStr};
use pest::iterators::Pair;

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum EntityIdError {
    #[error("Invalid address")]
    InvalidAddress,
    #[error("Invalid tx hash")]
    InvalidTxHash,
    #[error("Invalid block number")]
    InvalidBlockNumber,
    #[error("Unable resolve ENS name")]
    EnsResolution,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum EntityId {
    Block(BlockRange),
    Transaction(FixedBytes<32>),
    Account(NameOrAddress),
}

impl Display for EntityId {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            EntityId::Block(block_range) => write!(f, "{:?}", block_range),
            EntityId::Transaction(tx_hash) => write!(f, "TransactionHash({})", tx_hash),
            EntityId::Account(name_or_address) => write!(f, "{:?}", name_or_address),
        }
    }
}

impl<'a> TryFrom<Pair<'a, Rule>> for EntityId {
    type Error = Box<dyn Error>;

    fn try_from(pair: Pair<'a, Rule>) -> Result<Self, Self::Error> {
        match pair.as_rule() {
            Rule::account_id => {
                let account_id = pair.as_str().trim();
                // TODO: We shouldn't need to call `trim()` here, but the parser is
                // adding an extra whitespace when entity_id is block number.
                // The grammar and productions should be double checked.
                if account_id.starts_with("0x") {
                    if account_id.len() == 42 {
                        let address = Address::from_str(account_id).map_err(|_| "Invalid address")?;
                        let address = NameOrAddress::Address(address);
                        Ok(EntityId::Account(address))
                    } else {
                        Err(EntityIdError::InvalidAddress.into())
                    }
                } else if account_id.ends_with(".eth") {
                    let ens = NameOrAddress::Name(account_id.to_string());
                    Ok(EntityId::Account(ens))
                } else {
                    Err(EntityIdError::InvalidAddress.into())
                }
            }
            Rule::block_id => {
                let block_id = pair.as_str().trim();
                let (start, end) = match block_id.split_once(":") {
                    Some((start, end)) => {
                        let start = parse_block_number_or_tag(start)?;
                        let end = parse_block_number_or_tag(end)?;
                        (start, Some(end))
                    }
                    None => parse_block_number_or_tag(block_id).map(|start| (start, None))?,
                };

                Ok(EntityId::Block(BlockRange::new(start, end)))
            }
            Rule::tx_id => {
                let tx_id = pair.as_str().trim();
                if tx_id.len() == 66 {
                    let tx_hash = FixedBytes::from_str(tx_id).map_err(|_| "Invalid tx hash")?;
                    Ok(EntityId::Transaction(tx_hash))
                } else {
                    Err(EntityIdError::InvalidTxHash.into())
                }
            }
            _ => Err(Box::new(ParserError::UnexpectedToken(pair.as_str().to_string()))),
        }
    }
}

pub fn parse_block_number_or_tag(id: &str) -> Result<BlockNumberOrTag, EntityIdError> {
    match id.parse::<u64>() {
        Ok(id) => Ok(BlockNumberOrTag::Number(id)),
        Err(_) => id
            .parse::<BlockNumberOrTag>()
            .map_err(|_| EntityIdError::InvalidBlockNumber),
    }
}