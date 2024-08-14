use alloy::{
    eips::BlockNumberOrTag,
    rpc::types::Filter,
    primitives::Address,
};
use std::error::Error;

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum EntityFilter {
    BlockRange(BlockRange),
    LogBlockRange(BlockRange),
    LogEmitterAddress(Address),
    Transaction(),
    Account(),

}

// TODO: return instance of Error trait instead of &'static str
impl TryFrom<&str> for EntityFilter {
    type Error = Box<dyn Error>;

    fn try_from(id: &str) -> Result<Self, Self::Error> {
        let (start, end) = match id.split_once(":") {
            Some((start, end)) => {
                let start = parse_block_number_or_tag(start)?;
                let end = parse_block_number_or_tag(end)?;
                (start, Some(end))
            }
            None => parse_block_number_or_tag(id).map(|start| (start, None))?,
        };

        Ok(EntityFilter::BlockRange(BlockRange { start, end }))
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