use alloy::{
    eips::BlockNumberOrTag,
    rpc::types::Filter,
    primitives::Address,
};
use std::error::Error;
use pest::iterators::Pair;
use crate::interpreter::frontend::parser::{ParserError, Rule};

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum EntityFilter {
    BlockRange(BlockRange),
    LogBlockRange(BlockRange),
    LogEmitterAddress(Address),
    Transaction(),
    Account(),

}

impl<'a> TryFrom<Pair<'a, Rule>> for EntityFilter {
    type Error = Box<dyn Error>;

    fn try_from(pair:Pair<'a, Rule>) -> Result<Self, Self::Error> {
        match pair.as_rule() {
            Rule::address_filter => {
                let tochecksum = pair.as_str().trim_start_matches("address ");
                let address = Address::parse_checksummed(tochecksum, None)
                    .map_err(|e| format!("{}: {}", e, tochecksum))?;
                Ok(EntityFilter::LogEmitterAddress(address))
            },
            Rule::blockrange_filter => {
                let range = pair.as_str().trim_start_matches("block ");
                match range.split_once(":") {
                    //if ":" is present, we have an start and an end.
                    Some((start, end)) => {
                        let start = parse_block_number_or_tag(start)?;
                        let end = Some(parse_block_number_or_tag(end)?);
                        Ok(EntityFilter::LogBlockRange(BlockRange { start, end }))
                    }
                    //else we only have start.
                    None => {
                        let start = parse_block_number_or_tag(pair.as_str())?;
                        Ok(EntityFilter::LogBlockRange(BlockRange { start, end: None }))
                    }
                }
            }
            _ => Err(Box::new(ParserError::UnexpectedToken(pair.as_str().to_string()))),
        }
    }
}

impl EntityFilter {
    pub fn to_block_range(
        &self,
    ) -> Result<(BlockNumberOrTag, Option<BlockNumberOrTag>), EntityFilterError> {
        match self {
            EntityFilter::BlockRange(block_id) => Ok((block_id.start.clone(), block_id.end.clone())),
            _ => Err(EntityFilterError::InvalidBlockNumber),
        }
    }

    pub fn to_filter(&self, mut filter:Filter) -> Result<Filter, EntityFilterError> {
        match self {
            EntityFilter::LogBlockRange(block_id) => {
                filter = filter.from_block(block_id.start);
                if let Some(end) = block_id.end {
                    filter = filter.to_block(end);
                }
                Ok(filter)
            }
            EntityFilter::LogEmitterAddress(address) => {
                filter = filter.address(*address);
                Ok(filter)
            }
            _ => Err(EntityFilterError::InvalidBlockNumber),
        }
    }
}


#[derive(Debug, PartialEq, Eq, Clone)]
pub struct BlockRange {
    start: BlockNumberOrTag,
    end: Option<BlockNumberOrTag>,
}

impl BlockRange {
    pub fn new(start: BlockNumberOrTag, end: Option<BlockNumberOrTag>) -> Self {
        Self { start, end }
    }
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum EntityFilterError {
    #[error("Invalid block number")]
    InvalidBlockNumber,
}

fn parse_block_number_or_tag(id: &str) -> Result<BlockNumberOrTag, EntityFilterError> {
    match id.parse::<u64>() {
        Ok(id) => Ok(BlockNumberOrTag::Number(id)),
        Err(_) => id
            .parse::<BlockNumberOrTag>()
            .map_err(|_| EntityFilterError::InvalidBlockNumber),
    }
}