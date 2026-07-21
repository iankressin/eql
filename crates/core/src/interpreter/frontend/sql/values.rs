//! Coerces parsed SQL literals (`sqlparser::ast::Expr`) into domain values.
//!
//! `sql::prelex` rewrites bare hex literals and ENS names into single-quoted
//! SQL strings, and folds unit literals like `1 ether` into plain integers,
//! before the SQL parser ever sees them. Block tags such as `latest` are
//! deliberately left untouched by prelex, so they arrive as bare
//! `Expr::Identifier`s. The functions below account for both shapes where
//! relevant.

use super::EqlSqlError;
use crate::common::ens::NameOrAddress;
use alloy::eips::BlockNumberOrTag;
use alloy::primitives::{Address, B256, U256};
use sqlparser::ast::{Expr, Value};
use std::str::FromStr;

/// Extracts the text of a string literal or a bare identifier.
pub fn expr_as_string(expr: &Expr) -> Result<String, EqlSqlError> {
    match expr {
        Expr::Value(Value::SingleQuotedString(s)) => Ok(s.clone()),
        Expr::Identifier(ident) => Ok(ident.value.clone()),
        other => Err(EqlSqlError::Validation(format!(
            "expected a string or identifier, got {other}"
        ))),
    }
}

pub fn parse_address(expr: &Expr) -> Result<Address, EqlSqlError> {
    let s = expr_as_string(expr)?;
    Address::from_str(&s)
        .map_err(|e| EqlSqlError::Validation(format!("invalid address '{s}': {e}")))
}

pub fn parse_name_or_address(expr: &Expr) -> Result<NameOrAddress, EqlSqlError> {
    let s = expr_as_string(expr)?;
    if s.ends_with(".eth") {
        Ok(NameOrAddress::Name(s))
    } else {
        Ok(NameOrAddress::Address(Address::from_str(&s).map_err(
            |e| EqlSqlError::Validation(format!("invalid address '{s}': {e}")),
        )?))
    }
}

pub fn parse_b256(expr: &Expr) -> Result<B256, EqlSqlError> {
    let s = expr_as_string(expr)?;
    B256::from_str(&s).map_err(|e| EqlSqlError::Validation(format!("invalid hash '{s}': {e}")))
}

fn number_text(expr: &Expr) -> Result<String, EqlSqlError> {
    match expr {
        Expr::Value(Value::Number(n, _)) => Ok(n.clone()),
        other => Err(EqlSqlError::Validation(format!(
            "expected a number, got {other}"
        ))),
    }
}

pub fn parse_u8(expr: &Expr) -> Result<u8, EqlSqlError> {
    let s = number_text(expr)?;
    s.parse()
        .map_err(|e| EqlSqlError::Validation(format!("invalid number '{s}': {e}")))
}

pub fn parse_u64(expr: &Expr) -> Result<u64, EqlSqlError> {
    let s = number_text(expr)?;
    s.parse()
        .map_err(|e| EqlSqlError::Validation(format!("invalid number '{s}': {e}")))
}

pub fn parse_u128(expr: &Expr) -> Result<u128, EqlSqlError> {
    let s = number_text(expr)?;
    s.parse()
        .map_err(|e| EqlSqlError::Validation(format!("invalid number '{s}': {e}")))
}

pub fn parse_u256(expr: &Expr) -> Result<U256, EqlSqlError> {
    let s = number_text(expr)?;
    U256::from_str(&s).map_err(|e| EqlSqlError::Validation(format!("invalid number '{s}': {e}")))
}

pub fn parse_bool(expr: &Expr) -> Result<bool, EqlSqlError> {
    match expr {
        Expr::Value(Value::Boolean(b)) => Ok(*b),
        other => Err(EqlSqlError::Validation(format!(
            "expected true/false, got {other}"
        ))),
    }
}

pub fn parse_block_number_or_tag(expr: &Expr) -> Result<BlockNumberOrTag, EqlSqlError> {
    match expr {
        Expr::Value(Value::Number(_, _)) => Ok(BlockNumberOrTag::Number(parse_u64(expr)?)),
        Expr::Identifier(ident) => match ident.value.to_ascii_lowercase().as_str() {
            "latest" => Ok(BlockNumberOrTag::Latest),
            "earliest" => Ok(BlockNumberOrTag::Earliest),
            "pending" => Ok(BlockNumberOrTag::Pending),
            "finalized" => Ok(BlockNumberOrTag::Finalized),
            "safe" => Ok(BlockNumberOrTag::Safe),
            other => Err(EqlSqlError::Validation(format!(
                "invalid block tag '{other}'; expected latest, earliest, pending, finalized or safe"
            ))),
        },
        other => Err(EqlSqlError::Validation(format!(
            "expected a block number or tag, got {other}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlparser::ast::{Expr, Ident, UnaryOperator, Value};

    fn s(v: &str) -> Expr {
        Expr::Value(Value::SingleQuotedString(v.into()))
    }
    fn n(v: &str) -> Expr {
        Expr::Value(Value::Number(v.into(), false))
    }
    fn ident(v: &str) -> Expr {
        Expr::Identifier(Ident::new(v))
    }
    /// Mirrors how the real parser represents a negative numeric literal:
    /// `Expr::UnaryOp { op: Minus, .. }`, never a `Value::Number` with a
    /// leading `-` (see `Parser::parse_number`).
    fn neg(v: &str) -> Expr {
        Expr::UnaryOp {
            op: UnaryOperator::Minus,
            expr: Box::new(n(v)),
        }
    }

    #[test]
    fn parses_addresses_and_ens() {
        assert!(matches!(
            parse_name_or_address(&s("0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045")).unwrap(),
            crate::common::ens::NameOrAddress::Address(_)
        ));
        assert!(matches!(
            parse_name_or_address(&s("vitalik.eth")).unwrap(),
            crate::common::ens::NameOrAddress::Name(_)
        ));
    }

    #[test]
    fn parses_block_tags_and_numbers() {
        use alloy::eips::BlockNumberOrTag;
        assert_eq!(
            parse_block_number_or_tag(&ident("latest")).unwrap(),
            BlockNumberOrTag::Latest
        );
        assert_eq!(
            parse_block_number_or_tag(&ident("finalized")).unwrap(),
            BlockNumberOrTag::Finalized
        );
        assert_eq!(
            parse_block_number_or_tag(&n("100")).unwrap(),
            BlockNumberOrTag::Number(100)
        );
    }

    #[test]
    fn parses_numbers() {
        assert_eq!(parse_u64(&n("42")).unwrap(), 42);
        assert_eq!(
            parse_u256(&n("1000000000000000000")).unwrap().to_string(),
            "1000000000000000000"
        );
    }

    #[test]
    fn wrong_shapes_error() {
        assert!(parse_address(&s("not-hex")).is_err());
        assert!(parse_u64(&s("abc")).is_err());
        assert!(parse_block_number_or_tag(&ident("newest")).is_err());
    }

    #[test]
    fn parses_hashes() {
        let hash = "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
        assert_eq!(parse_b256(&s(hash)).unwrap(), B256::from_str(hash).unwrap());
    }

    #[test]
    fn rejects_bad_hashes() {
        // Right shape (hex string), wrong length: a 20-byte address is not
        // a valid 32-byte hash.
        assert!(parse_b256(&s("0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045")).is_err());
        // Not hex at all.
        assert!(parse_b256(&s("not-hex")).is_err());
    }

    #[test]
    fn parses_booleans() {
        // DuckDB's dialect represents boolean literals as `Value::Boolean`,
        // the same shape `sqlparser::Parser::parse_value` produces for the
        // `TRUE`/`FALSE` keywords.
        assert!(parse_bool(&Expr::Value(Value::Boolean(true))).unwrap());
        assert!(!parse_bool(&Expr::Value(Value::Boolean(false))).unwrap());
    }

    #[test]
    fn rejects_non_boolean() {
        assert!(parse_bool(&s("true")).is_err());
        assert!(parse_bool(&n("1")).is_err());
    }

    #[test]
    fn rejects_negative_numbers() {
        // The real parser never emits a negative `Value::Number` (unary
        // minus wraps the literal in an `Expr::UnaryOp` instead), so these
        // are rejected simply because the shape isn't `Expr::Value(Number)`.
        assert!(parse_u8(&neg("5")).is_err());
        assert!(parse_u64(&neg("5")).is_err());
        assert!(parse_u128(&neg("5")).is_err());
        assert!(parse_u256(&neg("5")).is_err());
    }

    #[test]
    fn rejects_overflow() {
        assert!(parse_u8(&n("256")).is_err()); // u8::MAX + 1
        assert!(parse_u64(&n("18446744073709551616")).is_err()); // u64::MAX + 1
        assert!(parse_u128(&n("340282366920938463463374607431768211456")).is_err()); // u128::MAX + 1
        assert!(parse_u256(&n(
            "115792089237316195423570985008687907853269984665640564039457584007913129639936"
        ))
        .is_err()); // U256::MAX + 1
    }

    #[test]
    fn rejects_hex_string_where_number_expected() {
        // Hex literals reach us as single-quoted strings (via prelex), not
        // as `Value::Number`, so a decimal-only parser must reject them.
        assert!(parse_u64(&s("0x2a")).is_err());
        assert!(parse_u256(&s("0x2a")).is_err());
    }

    #[test]
    fn rejects_decimal_point() {
        assert!(parse_u8(&n("1.5")).is_err());
        assert!(parse_u64(&n("1.5")).is_err());
        assert!(parse_u128(&n("1.5")).is_err());
        assert!(parse_u256(&n("1.5")).is_err());
    }
}
