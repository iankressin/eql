use super::{
    block::{BlockId, BlockRange},
    entity_id::{parse_block_number_or_tag, EntityIdError},
    filters::{
        ComparisonFilterError, EqualityFilter, EqualityFilterError, Filter, FilterError, FilterType,
    },
    query_result::TransactionQueryRes,
};
use crate::interpreter::frontend::parser::Rule;
use alloy::{
    hex::FromHexError,
    primitives::{Address, AddressError, B256, U256},
};
use eql_macros::EnumVariants;
use pest::iterators::{Pair, Pairs};
use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[derive(Debug, PartialEq)]
pub struct Transaction {
    ids: Option<Vec<B256>>,
    filters: Option<Vec<TransactionFilter>>,
    fields: Vec<TransactionField>,
}

impl Transaction {
    pub fn new(
        ids: Option<Vec<B256>>,
        filters: Option<Vec<TransactionFilter>>,
        fields: Vec<TransactionField>,
    ) -> Self {
        Self {
            ids,
            filters,
            fields,
        }
    }

    pub fn ids(&self) -> Option<&Vec<B256>> {
        self.ids.as_ref()
    }

    pub fn fields(&self) -> &Vec<TransactionField> {
        &self.fields
    }

    pub fn filters(&self) -> Option<&Vec<TransactionFilter>> {
        self.filters.as_ref()
    }

    pub fn get_block_id_filter(&self) -> Result<&BlockId, TransactionFilterError> {
        self.filters
            .as_ref()
            .and_then(|filters| {
                filters
                    .iter()
                    .find(|f| matches!(f, TransactionFilter::BlockId(_)))
                    .and_then(|filter| filter.as_block_id().ok())
            })
            .ok_or(TransactionFilterError::InvalidBlockIdFilter)
    }

    pub fn filter(&self, tx: &TransactionQueryRes) -> bool {
        if let Some(filters) = &self.filters {
            filters.iter().all(|filter| match filter {
                TransactionFilter::Type(t) => tx.r#type.as_ref().is_some_and(|v| t.compare(v)),
                TransactionFilter::Hash(h) => tx.hash.as_ref().is_some_and(|v| h.compare(v)),
                TransactionFilter::From(f) => {
                    tx.from_address.as_ref().is_some_and(|v| f.compare(v))
                }
                TransactionFilter::To(t) => tx.to_address.as_ref().is_some_and(|v| t.compare(v)),
                TransactionFilter::Data(d) => tx.data.as_ref().is_some_and(|v| d.compare(v)),
                TransactionFilter::Value(v) => tx.value.as_ref().is_some_and(|n| v.compare(n)),
                TransactionFilter::GasPrice(gp) => {
                    tx.gas_price.as_ref().is_some_and(|v| gp.compare(v))
                }
                TransactionFilter::GasLimit(g) => {
                    tx.gas_limit.as_ref().is_some_and(|v| g.compare(v))
                }
                TransactionFilter::EffectiveGasPrice(egp) => tx
                    .effective_gas_price
                    .as_ref()
                    .is_some_and(|v| egp.compare(v)),
                TransactionFilter::ChainId(cid) => {
                    tx.chain_id.as_ref().is_some_and(|v| cid.compare(v))
                }
                TransactionFilter::Status(s) => tx.status.as_ref().is_some_and(|v| s.compare(v)),
                TransactionFilter::V(v) => tx.v.as_ref().is_some_and(|n| v.compare(n)),
                TransactionFilter::R(r) => tx.r.as_ref().is_some_and(|v| r.compare(v)),
                TransactionFilter::S(s) => tx.s.as_ref().is_some_and(|v| s.compare(v)),
                TransactionFilter::MaxFeePerBlobGas(mfbg) => tx
                    .max_fee_per_blob_gas
                    .as_ref()
                    .is_some_and(|v| mfbg.compare(v)),
                TransactionFilter::MaxFeePerGas(mfg) => {
                    tx.max_fee_per_gas.as_ref().is_some_and(|v| mfg.compare(v))
                }
                TransactionFilter::MaxPriorityFeePerGas(mpfpg) => tx
                    .max_priority_fee_per_gas
                    .as_ref()
                    .is_some_and(|v| mpfpg.compare(v)),
                TransactionFilter::YParity(yp) => {
                    tx.y_parity.as_ref().is_some_and(|v| yp.compare(v))
                }
                // TODO: once we have implemented the transaction receipt fields, should validate the block id
                TransactionFilter::BlockId(_) => true,
            })
        } else {
            true
        }
    }

    pub fn has_block_filter(&self) -> bool {
        match self.filters() {
            Some(filters) => filters
                .iter()
                .any(|f| matches!(f, TransactionFilter::BlockId(BlockId::Range(_)))),
            None => false,
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum TransactionError {
    #[error("Unexpected token {0} for transaction")]
    UnexpectedToken(String),
    #[error(transparent)]
    EntityIdError(#[from] EntityIdError),
    #[error(transparent)]
    FromHexError(#[from] FromHexError),
    #[error(transparent)]
    AddressError(#[from] AddressError),
    #[error(transparent)]
    TransactionFieldError(#[from] TransactionFieldError),
    #[error(transparent)]
    TransactionFilterError(#[from] TransactionFilterError),
}

impl TryFrom<Pairs<'_, Rule>> for Transaction {
    type Error = TransactionError;

    fn try_from(pairs: Pairs<'_, Rule>) -> Result<Self, Self::Error> {
        let mut ids: Option<Vec<B256>> = None;
        let mut filter: Option<Vec<TransactionFilter>> = None;
        let mut fields: Vec<TransactionField> = vec![];

        for pair in pairs {
            match pair.as_rule() {
                Rule::tx_id => {
                    if let Some(ids) = ids.as_mut() {
                        ids.push(B256::from_str(pair.as_str())?);
                    } else {
                        ids = Some(vec![B256::from_str(pair.as_str())?]);
                    }
                }
                Rule::tx_filter => {
                    let next_filter = pair.into_inner().next().unwrap();
                    if let Some(filter) = filter.as_mut() {
                        filter.push(TransactionFilter::try_from(next_filter)?);
                    } else {
                        filter = Some(vec![TransactionFilter::try_from(next_filter)?]);
                    }
                }
                Rule::tx_fields => {
                    let inner_pairs = pair.into_inner();

                    if let Some(pair) = inner_pairs.peek() {
                        if pair.as_rule() == Rule::wildcard {
                            fields = TransactionField::all_variants().to_vec();
                            continue;
                        }
                    }
                    fields = inner_pairs
                        .map(|pair| TransactionField::try_from(pair.as_str()))
                        .collect::<Result<Vec<TransactionField>, TransactionFieldError>>()?;
                }
                _ => {
                    return Err(TransactionError::UnexpectedToken(pair.as_str().to_string()));
                }
            }
        }

        Ok(Transaction {
            ids,
            filters: filter,
            fields,
        })
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize, EnumVariants)]
pub enum TransactionField {
    Type,
    Hash,
    BlockNumber,
    From,
    To,
    Data,
    Value,
    GasPrice,
    GasLimit,
    EffectiveGasPrice,
    Status,
    ChainId,
    V,
    R,
    S,
    MaxFeePerBlobGas,
    MaxFeePerGas,
    MaxPriorityFeePerGas,
    YParity,
    Chain,
    AuthorizationList,
}

impl std::fmt::Display for TransactionField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransactionField::Type => write!(f, "type"),
            TransactionField::Hash => write!(f, "hash"),
            TransactionField::BlockNumber => write!(f, "block_number"),
            TransactionField::From => write!(f, "from_address"),
            TransactionField::To => write!(f, "to_address"),
            TransactionField::Data => write!(f, "data"),
            TransactionField::Value => write!(f, "value"),
            TransactionField::GasPrice => write!(f, "gas_price"),
            TransactionField::GasLimit => write!(f, "gas_limit"),
            TransactionField::EffectiveGasPrice => write!(f, "effective_gas_price"),
            TransactionField::Status => write!(f, "status"),
            TransactionField::ChainId => write!(f, "chain_id"),
            TransactionField::V => write!(f, "v"),
            TransactionField::R => write!(f, "r"),
            TransactionField::S => write!(f, "s"),
            TransactionField::MaxFeePerBlobGas => write!(f, "max_fee_per_blob_gas"),
            TransactionField::MaxFeePerGas => write!(f, "max_fee_per_gas"),
            TransactionField::MaxPriorityFeePerGas => write!(f, "max_priority_fee_per_gas"),
            TransactionField::YParity => write!(f, "y_parity"),
            TransactionField::Chain => write!(f, "chain"),
            TransactionField::AuthorizationList => write!(f, "authorization_list"),
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum TransactionFieldError {
    #[error("Invalid transaction field: {0}")]
    InvalidTransactionField(String),
}

// TODO: this can possibly be removed as we're using TryFrom<Pair<'_, Rule>> for TransactionField
impl TryFrom<&str> for TransactionField {
    type Error = TransactionFieldError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "type" => Ok(TransactionField::Type),
            "hash" => Ok(TransactionField::Hash),
            "block_number" => Ok(TransactionField::BlockNumber),
            "from_address" | "from" => Ok(TransactionField::From),
            "to_address" | "to" => Ok(TransactionField::To),
            "data" => Ok(TransactionField::Data),
            "value" => Ok(TransactionField::Value),
            "gas_price" => Ok(TransactionField::GasPrice),
            "gas_limit" => Ok(TransactionField::GasLimit),
            "effective_gas_price" => Ok(TransactionField::EffectiveGasPrice),
            "status" => Ok(TransactionField::Status),
            "chain_id" => Ok(TransactionField::ChainId),
            "v" => Ok(TransactionField::V),
            "r" => Ok(TransactionField::R),
            "s" => Ok(TransactionField::S),
            "max_fee_per_blob_gas" => Ok(TransactionField::MaxFeePerBlobGas),
            "max_fee_per_gas" => Ok(TransactionField::MaxFeePerGas),
            "max_priority_fee_per_gas" => Ok(TransactionField::MaxPriorityFeePerGas),
            "y_parity" => Ok(TransactionField::YParity),
            "chain" => Ok(TransactionField::Chain),
            "authorization_list" => Ok(TransactionField::AuthorizationList),
            invalid_field => Err(TransactionFieldError::InvalidTransactionField(
                invalid_field.to_string(),
            )),
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum TransactionFilterError {
    #[error("Invalid transaction filter property: {0}")]
    InvalidTransactionFilterProperty(String),
    #[error("Missing operator in filter")]
    MissingOperator,
    #[error(transparent)]
    EntityIdError(#[from] EntityIdError),
    #[error(transparent)]
    FromHexError(#[from] FromHexError),
    #[error("BlockId filter is not valid")]
    InvalidBlockIdFilter,
    #[error(transparent)]
    ComparisonFilterError(#[from] ComparisonFilterError),
    #[error(transparent)]
    FilterError(#[from] FilterError),
}

#[derive(Debug, PartialEq)]
pub enum TransactionFilter {
    Type(EqualityFilter<u8>),
    Hash(EqualityFilter<B256>),
    From(EqualityFilter<Address>),
    To(EqualityFilter<Address>),
    Data(EqualityFilter<alloy::primitives::Bytes>),
    Value(FilterType<U256>),
    GasPrice(FilterType<u128>),
    GasLimit(FilterType<u64>),
    EffectiveGasPrice(FilterType<u128>),
    ChainId(EqualityFilter<u64>),
    BlockId(BlockId),
    Status(EqualityFilter<bool>),
    V(EqualityFilter<bool>),
    R(EqualityFilter<U256>),
    S(EqualityFilter<U256>),
    MaxFeePerBlobGas(FilterType<u128>),
    MaxFeePerGas(FilterType<u128>),
    MaxPriorityFeePerGas(FilterType<u128>),
    YParity(EqualityFilter<bool>),
}

impl TransactionFilter {
    pub fn as_block_id(&self) -> Result<&BlockId, TransactionFilterError> {
        if let TransactionFilter::BlockId(block_id) = self {
            Ok(block_id)
        } else {
            Err(TransactionFilterError::InvalidBlockIdFilter)
        }
    }

    // Helper function to parse filter components
    fn parse_filter<'a, T, F>(
        pair: Pair<'a, Rule>,
        value_parser: F,
        constructor: impl FnOnce(FilterType<T>) -> TransactionFilter,
    ) -> Result<TransactionFilter, TransactionFilterError>
    where
        F: FnOnce(&str) -> T,
        FilterType<T>: TryFrom<(Pair<'a, Rule>, T), Error = FilterError>,
    {
        let mut inner_pair = pair.into_inner();
        let operator = inner_pair.next();

        match operator {
            Some(op) => {
                let value = value_parser(inner_pair.as_str().trim());
                Ok(constructor(FilterType::try_from((op, value))?))
            }
            None => Err(TransactionFilterError::MissingOperator),
        }
    }

    /// Helper function to parse equality filter components
    fn parse_equality_filter<'a, T, F>(
        pair: Pair<'a, Rule>,
        value_parser: F,
        constructor: impl FnOnce(EqualityFilter<T>) -> TransactionFilter,
    ) -> Result<TransactionFilter, TransactionFilterError>
    where
        F: FnOnce(&str) -> T,
        EqualityFilter<T>: TryFrom<(Pair<'a, Rule>, T), Error = EqualityFilterError>,
    {
        let mut inner_pair = pair.into_inner();
        let operator = inner_pair.next();

        match operator {
            Some(op) => {
                let value = value_parser(inner_pair.as_str().trim());
                let filter =
                    EqualityFilter::try_from((op, value)).map_err(|e| FilterError::from(e))?;
                Ok(constructor(filter))
            }
            None => Err(TransactionFilterError::MissingOperator),
        }
    }

    // Implementation using the helper
    fn try_from(pair: Pair<'_, Rule>) -> Result<Self, TransactionFilterError> {
        match pair.as_rule() {
            Rule::blockrange_filter => {
                let range = pair
                    .as_str()
                    .trim_start_matches("block")
                    .trim_start_matches(|c: char| c.is_whitespace() || c == '=')
                    .trim();

                let (start, end) = match range.split_once(':') {
                    // if ":" is present, we have a start and an end
                    Some((start, end)) => (
                        parse_block_number_or_tag(start)?,
                        Some(parse_block_number_or_tag(end)?),
                    ),
                    // else we only have start
                    None => (parse_block_number_or_tag(range)?, None),
                };
                Ok(TransactionFilter::BlockId(BlockId::Range(BlockRange::new(
                    start, end,
                ))))
            }
            Rule::type_filter_type => Self::parse_equality_filter(
                pair,
                |s| s.parse::<u8>().unwrap(),
                TransactionFilter::Type,
            ),
            Rule::value_filter_type => Self::parse_filter(
                pair,
                |s| U256::from_str(s).unwrap(),
                TransactionFilter::Value,
            ),
            Rule::gas_price_filter_type => Self::parse_filter(
                pair,
                |s| s.parse::<u128>().unwrap(),
                TransactionFilter::GasPrice,
            ),
            Rule::gas_limit_filter_type => Self::parse_filter(
                pair,
                |s| s.parse::<u64>().unwrap(),
                TransactionFilter::GasLimit,
            ),
            Rule::max_fee_per_blob_gas_filter_type => Self::parse_filter(
                pair,
                |s| s.parse::<u128>().unwrap(),
                TransactionFilter::MaxFeePerBlobGas,
            ),
            Rule::max_fee_per_gas_filter_type => Self::parse_filter(
                pair,
                |s| s.parse::<u128>().unwrap(),
                TransactionFilter::MaxFeePerGas,
            ),
            Rule::max_priority_fee_per_gas_filter_type => Self::parse_filter(
                pair,
                |s| s.parse::<u128>().unwrap(),
                TransactionFilter::MaxPriorityFeePerGas,
            ),
            Rule::status_filter_type => {
                let mut inner_pair = pair.into_inner();
                let operator = inner_pair.next().unwrap();
                let value = inner_pair.as_str().trim();

                Ok(TransactionFilter::Status(
                    EqualityFilter::try_from((operator, value == "true")).unwrap(),
                ))
            }
            Rule::from_filter_type => Self::parse_equality_filter(
                pair,
                |s| Address::from_str(s).unwrap(),
                TransactionFilter::From,
            ),
            Rule::to_filter_type => Self::parse_equality_filter(
                pair,
                |s| Address::from_str(s).unwrap(),
                TransactionFilter::To,
            ),
            Rule::data_filter_type => {
                let mut inner_pair = pair.into_inner();
                let operator = inner_pair.next().unwrap();
                let value = alloy::primitives::Bytes::from_str(inner_pair.as_str()).unwrap();
                let result = EqualityFilter::try_from((operator, value)).unwrap();

                Ok(TransactionFilter::Data(result))
            }
            Rule::y_parity_filter_type => {
                let mut inner_pair = pair.into_inner();
                let operator = inner_pair.next().unwrap();
                let value = inner_pair.as_str();

                Ok(TransactionFilter::YParity(
                    EqualityFilter::try_from((operator, value == "true")).unwrap(),
                ))
            }
            _ => Err(TransactionFilterError::InvalidTransactionFilterProperty(
                pair.as_str().to_string(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use alloy::eips::BlockNumberOrTag;

    use super::*;
    use crate::common::filters::ComparisonFilter;

    #[test]
    fn test_return_true_if_tx_passes_all_filters() {
        let value = U256::from(1000000000);

        let tx_query_res = TransactionQueryRes {
            value: Some(value),
            ..Default::default()
        };

        let transaction = Transaction::new(
            None,
            Some(vec![TransactionFilter::Value(FilterType::Comparison(
                ComparisonFilter::Lte(value),
            ))]),
            vec![TransactionField::Hash],
        );

        assert_eq!(true, transaction.filter(&tx_query_res));
    }

    #[test]
    fn test_return_false_if_tx_does_not_pass_any_filters() {
        let tx_query_res = TransactionQueryRes {
            value: Some(U256::from(1)),
            r#type: Some(2),
            ..Default::default()
        };

        // let filter = FilterType::Comparison(ComparisonFilter::Gte(U256::from(1000000000)));

        // GET type FROM tx WHERE block = 45087:45187, type = 4 ON mekong
        let transaction = Transaction::new(
            None,
            Some(vec![
                TransactionFilter::BlockId(BlockId::Range(BlockRange::new(
                    BlockNumberOrTag::Number(45087),
                    Some(BlockNumberOrTag::Number(45187)),
                ))),
                TransactionFilter::Type(EqualityFilter::Eq(4)),
            ]),
            vec![TransactionField::Type],
        );

        assert_eq!(false, transaction.filter(&tx_query_res));
    }

    #[test]
    fn test_nullable_transaction_filter_is_false_instead_of_panicking() {
        let transaction = Transaction::new(
            None,
            Some(vec![TransactionFilter::To(EqualityFilter::Eq(
                Address::ZERO,
            ))]),
            vec![TransactionField::AuthorizationList],
        );

        assert!(!transaction.filter(&TransactionQueryRes::default()));
    }

    #[test]
    fn transaction_field_display_uses_renamed_columns() {
        assert_eq!(TransactionField::From.to_string(), "from_address");
        assert_eq!(TransactionField::To.to_string(), "to_address");
        assert_eq!(TransactionField::BlockNumber.to_string(), "block_number");
    }

    #[test]
    fn transaction_field_parses_renamed_columns() {
        assert_eq!(
            TransactionField::try_from("from_address").unwrap(),
            TransactionField::From
        );
        assert_eq!(
            TransactionField::try_from("to_address").unwrap(),
            TransactionField::To
        );
        assert_eq!(
            TransactionField::try_from("block_number").unwrap(),
            TransactionField::BlockNumber
        );
        // legacy spellings still parse (used by the legacy translator)
        assert_eq!(
            TransactionField::try_from("from").unwrap(),
            TransactionField::From
        );
        assert_eq!(
            TransactionField::try_from("to").unwrap(),
            TransactionField::To
        );
    }
}
