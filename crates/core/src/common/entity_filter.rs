use alloy::{
    eips::BlockNumberOrTag,
    rpc::types::Filter,
    primitives::{Address, B256},
};
use std::error::Error;
use pest::iterators::Pair;
use crate::interpreter::frontend::parser::{ParserError, Rule};

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum EntityFilter {
    BlockRange(BlockRange),
    LogBlockRange(BlockRange),
    LogBlockHash(B256),
    LogEmitterAddress(Address),
    LogEventSignature(String),
    LogTopic0(B256),
    LogTopic1(B256),    
    LogTopic2(B256),
    LogTopic3(B256),
    Transaction(),
    Account(),

}

impl<'a> TryFrom<Pair<'a, Rule>> for EntityFilter {
    type Error = Box<dyn Error>;

    fn try_from(pair: Pair<'a, Rule>) -> Result<Self, Self::Error> {
        match pair.as_rule() {
            Rule::address_filter => {
                let tochecksum = pair.as_str().trim_start_matches("address ").trim();
                let address = Address::parse_checksummed(tochecksum, None)
                    .map_err(|e| format!("{}: {}", e, tochecksum))?;
                Ok(EntityFilter::LogEmitterAddress(address))
            },
            Rule::blockrange_filter => {
                let range = pair.as_str().trim_start_matches("block ").trim();
                let (start, end) = match range.split_once(":") {
                    //if ":" is present, we have an start and an end.
                    Some((start, end)) => (
                        parse_block_number_or_tag(start)?,
                        Some(parse_block_number_or_tag(end)?),
                    ),
                    //else we only have start.
                    None => (
                        parse_block_number_or_tag(range)?,
                        None,
                    ),
                };
                Ok(EntityFilter::LogBlockRange(BlockRange { start, end }))
            }
            Rule::blockhash_filter => {
                let hash = pair.as_str().trim_start_matches("blockhash ").trim_start_matches("block_hash ").trim();
                let hash = hash.parse::<B256>()?;
                Ok(EntityFilter::LogBlockHash(hash))
            }
            Rule::topic0_filter => {
                let topic = pair.as_str().trim_start_matches("topic0 ").trim();
                let topic = topic.parse::<B256>()?;
                Ok(EntityFilter::LogTopic0(topic))
            }
            Rule::topic1_filter => {
                let topic = pair.as_str().trim_start_matches("topic1 ").trim();
                let topic = topic.parse::<B256>()?;
                Ok(EntityFilter::LogTopic1(topic))
            }
            Rule::topic2_filter => {
                let topic = pair.as_str().trim_start_matches("topic2 ").trim();
                let topic = topic.parse::<B256>()?;
                Ok(EntityFilter::LogTopic2(topic))
            }
            Rule::topic3_filter => {
                let topic = pair.as_str().trim_start_matches("topic3 ").trim();
                let topic = topic.parse::<B256>()?;
                Ok(EntityFilter::LogTopic3(topic))
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
            EntityFilter::LogBlockRange(block_id) => Ok((block_id.start.clone(), block_id.end.clone())),
            _ => Err(EntityFilterError::InvalidBlockNumber),
        }
    }

    pub fn to_filter(&self, mut filter: Filter) -> Result<Filter, EntityFilterError> {
        match self {
            EntityFilter::LogBlockRange(block_id) => {
                filter = filter.from_block(block_id.start);
                filter = filter.to_block(block_id.end.unwrap_or(block_id.start)); // Use `unwrap_or` for conditional assignment
            }
            EntityFilter::LogBlockHash(hash) => filter = filter.at_block_hash(*hash),
            EntityFilter::LogEmitterAddress(address) => filter = filter.address(*address),
            EntityFilter::LogEventSignature(signature) => filter = filter.event(signature),
            EntityFilter::LogTopic0(topic_hash) => filter = filter.event_signature(*topic_hash),
            EntityFilter::LogTopic1(topic_hash) => filter = filter.topic1(*topic_hash),
            EntityFilter::LogTopic2(topic_hash) => filter = filter.topic2(*topic_hash),
            EntityFilter::LogTopic3(topic_hash) => filter = filter.topic3(*topic_hash),
            _ => return Err(EntityFilterError::InvalidBlockNumber), // Explicit return for error case
        }
    
        Ok(filter)
    }
}

//I should provide methods to change start and end instead of making the params pub.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct BlockRange {
    pub start: BlockNumberOrTag,
    pub end: Option<BlockNumberOrTag>,
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