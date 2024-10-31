use crate::interpreter::frontend::parser::Rule;
use pest::iterators::Pair;

pub trait Filter<T> {
    fn compare(&self, a: &T) -> bool;
}

#[derive(Debug, PartialEq)]
pub enum FilterType<T> {
    Equality(EqualityFilter<T>),
    Comparison(ComparisonFilter<T>),
}

impl<T> Filter<T> for FilterType<T>
where
    EqualityFilter<T>: Filter<T>,
    ComparisonFilter<T>: Filter<T>,
{
    fn compare(&self, a: &T) -> bool {
        match self {
            FilterType::Equality(filter) => filter.compare(a),
            FilterType::Comparison(filter) => filter.compare(a),
        }
    }
}

impl<'a, T> TryFrom<(Pair<'a, Rule>, T)> for FilterType<T>
where
    EqualityFilter<T>: TryFrom<(Pair<'a, Rule>, T), Error = EqualityFilterError>,
    ComparisonFilter<T>: TryFrom<(Pair<'a, Rule>, T), Error = ComparisonFilterError>,
{
    type Error = FilterError;

    fn try_from((pair, value): (Pair<'a, Rule>, T)) -> Result<Self, Self::Error> {
        match pair.as_rule() {
            Rule::equality_operators => {
                let filter = EqualityFilter::try_from((pair, value))?;
                Ok(FilterType::Equality(filter))
            }
            Rule::comparison_operators => {
                let filter = ComparisonFilter::try_from((pair, value))?;
                Ok(FilterType::Comparison(filter))
            }
            _ => Err(FilterError::InvalidFilter(pair.as_str().to_string())),
        }
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
        let inner_operator = operator.into_inner().next().ok_or_else(|| {
            EqualityFilterError::InvalidOperator("Missing operator in filter".to_string())
        })?;

        match inner_operator.as_rule() {
            Rule::eq_operator => Ok(Self::Eq(value)),
            Rule::neq_operator => Ok(Self::Neq(value)),
            _ => Err(EqualityFilterError::InvalidOperator(
                inner_operator.as_str().to_string(),
            )),
        }
    }
}

impl<T> Filter<T> for EqualityFilter<T>
where
    T: PartialEq,
{
    fn compare(&self, a: &T) -> bool {
        match self {
            EqualityFilter::Eq(value) => a == value,
            EqualityFilter::Neq(value) => a != value,
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

impl<T> Filter<T> for ComparisonFilter<T>
where
    T: PartialOrd,
{
    fn compare(&self, a: &T) -> bool {
        match self {
            ComparisonFilter::Gt(value) => a > value,
            ComparisonFilter::Gte(value) => a >= value,
            ComparisonFilter::Lt(value) => a < value,
            ComparisonFilter::Lte(value) => a <= value,
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ComparisonFilterError {
    #[error("Invalid operator {0}")]
    InvalidOperator(String),
    #[error("Missing operator in filter")]
    MissingOperator,
}

impl<T> TryFrom<(Pair<'_, Rule>, T)> for ComparisonFilter<T> {
    type Error = ComparisonFilterError;

    fn try_from((operator, value): (Pair<'_, Rule>, T)) -> Result<Self, Self::Error> {
        let inner_operator = operator
            .into_inner()
            .next()
            .ok_or_else(|| ComparisonFilterError::MissingOperator)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_equality_filter() {
        let filter = EqualityFilter::Eq(1);
        assert!(filter.compare(&1));
        assert!(!filter.compare(&2));
    }

    #[test]
    fn test_comparison_filter() {
        let filter = ComparisonFilter::Gt(1);
        assert!(filter.compare(&2));
        assert!(!filter.compare(&1));
        assert!(!filter.compare(&0));
    }

    #[test]
    fn test_filter_type_equality() {
        let filter = FilterType::Equality(EqualityFilter::Eq(1));
        assert!(filter.compare(&1));
        assert!(!filter.compare(&2));
    }

    #[test]
    fn test_filter_type_comparison() {
        let filter = FilterType::Comparison(ComparisonFilter::Lt(5));
        assert!(filter.compare(&3));
        assert!(!filter.compare(&5));
        assert!(!filter.compare(&7));
    }
}
