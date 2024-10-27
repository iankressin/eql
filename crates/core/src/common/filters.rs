use crate::interpreter::frontend::parser::Rule;
use pest::iterators::Pair;
use std::fmt::Debug;

pub trait FilterTrait<T>: Debug {
    fn compare(&self, a: T) -> bool;
}

#[derive(Debug)]
pub struct FullFilter<T: PartialEq + PartialOrd + Copy + 'static> {
    value: T,
    operator: Box<dyn FilterTrait<T>>,
}

impl<T: PartialEq + PartialOrd + Copy + 'static> FullFilter<T> {
    pub fn new(value: T, operator: Box<dyn FilterTrait<T>>) -> Self {
        Self { value, operator }
    }
}

impl<T: PartialEq + PartialOrd + Copy + 'static> PartialEq for FullFilter<T> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
            && self.operator.compare(other.value) == other.operator.compare(self.value)
    }
}

impl<T: PartialEq + PartialOrd + Copy + 'static> Eq for FullFilter<T> {}

impl<T: PartialEq + PartialOrd + Copy + Debug + 'static> FilterTrait<T> for FullFilter<T> {
    fn compare(&self, a: T) -> bool {
        self.operator.compare(a)
    }
}

impl<T: PartialEq + PartialOrd + Copy + Debug + 'static> TryFrom<(Pair<'_, Rule>, T)>
    for FullFilter<T>
{
    type Error = FilterError;

    fn try_from((pair, value): (Pair<'_, Rule>, T)) -> Result<Self, Self::Error> {
        let operator: Box<dyn FilterTrait<T>> = match pair.as_rule() {
            Rule::equality_operators => Box::new(EqualityFilter::try_from((pair, value))?),
            Rule::comparison_operators => Box::new(ComparisonFilter::try_from((pair, value))?),
            _ => return Err(FilterError::InvalidFilter(pair.as_str().to_string())),
        };

        Ok(Self { value, operator })
    }
}

#[derive(thiserror::Error, Debug)]
pub enum FilterError {
    #[error("Invalid filter {0}")]
    InvalidFilter(String),
    #[error(transparent)]
    EqualityFilterError(#[from] EqualityFilterError),
    #[error(transparent)]
    ComparisonFilterError(#[from] ComparisonFilterError),
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum EqualityFilter<T> {
    Eq(T),
    Neq(T),
}

#[derive(thiserror::Error, Debug)]
pub enum EqualityFilterError {
    #[error("Invalid operator {0}")]
    InvalidOperator(String),
}

impl<T> TryFrom<(Pair<'_, Rule>, T)> for EqualityFilter<T> {
    type Error = EqualityFilterError;

    fn try_from((operator, value): (Pair<'_, Rule>, T)) -> Result<Self, Self::Error> {
        let inner_operator = operator.into_inner().next().unwrap();

        match inner_operator.as_rule() {
            Rule::eq_operator => Ok(Self::Eq(value)),
            Rule::neq_operator => Ok(Self::Neq(value)),
            _ => Err(EqualityFilterError::InvalidOperator(
                inner_operator.as_str().to_string(),
            )),
        }
    }
}

impl<T: PartialEq + Debug> FilterTrait<T> for EqualityFilter<T> {
    fn compare(&self, a: T) -> bool {
        match self {
            EqualityFilter::Eq(value) => a == *value,
            EqualityFilter::Neq(value) => a != *value,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ComparisonFilter<T> {
    Gt(T),
    Gte(T),
    Lt(T),
    Lte(T),
}

impl<T: PartialOrd + Debug> FilterTrait<T> for ComparisonFilter<T> {
    fn compare(&self, a: T) -> bool {
        match self {
            ComparisonFilter::Gt(value) => a > *value,
            ComparisonFilter::Gte(value) => a >= *value,
            ComparisonFilter::Lt(value) => a < *value,
            ComparisonFilter::Lte(value) => a <= *value,
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ComparisonFilterError {
    #[error("Invalid operator {0}")]
    InvalidOperator(String),
}

impl<T> TryFrom<(Pair<'_, Rule>, T)> for ComparisonFilter<T> {
    type Error = ComparisonFilterError;

    fn try_from((operator, value): (Pair<'_, Rule>, T)) -> Result<Self, Self::Error> {
        let inner_operator = operator.into_inner().next().unwrap();
        match inner_operator.as_rule() {
            Rule::gt_operator => Ok(Self::Gt(value)),
            Rule::gte_operator => Ok(Self::Gte(value)),
            Rule::lt_operator => Ok(Self::Lt(value)),
            Rule::lte_operator => Ok(Self::Lte(value)),
            _ => Err(ComparisonFilterError::InvalidOperator(
                inner_operator.as_str().to_string(),
            )),
        }
    }
}
