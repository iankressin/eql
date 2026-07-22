pub mod legacy;
pub mod prelex;
pub mod schema;
pub mod translate;
pub mod values;
pub mod where_clause;

use crate::common::types::Expression;
use sqlparser::{dialect::DuckDbDialect, parser::Parser as SqlParser};

#[derive(thiserror::Error, Debug)]
pub enum EqlSqlError {
    #[error("SQL parse error: {0}")]
    Parse(String),
    #[error("{0} is not supported by EQL yet")]
    NotSupported(String),
    #[error("{0}")]
    Validation(String),
    #[error("EQL 2 uses SQL syntax. Equivalent:\n\n{suggestion}")]
    LegacySyntax { suggestion: String },
}

/// The single frontend entry point: turns SQL source text into the
/// `Expression`s the backend executes.
///
/// Checks `legacy::legacy_error` first — an old `GET ... FROM ... ON ...`
/// query is not valid SQL, so letting it fall through to `sqlparser` would
/// only ever produce a raw, unhelpful parse error. Catching it here first
/// means a user who hasn't migrated gets the EQL 2 equivalent instead.
pub fn parse_program(source: &str) -> Result<Vec<Expression>, EqlSqlError> {
    if let Some(err) = legacy::legacy_error(source) {
        return Err(err);
    }
    let prelexed = prelex::prelex(source)?;
    let statements = SqlParser::parse_sql(&DuckDbDialect {}, &prelexed)
        .map_err(|e| EqlSqlError::Parse(e.to_string()))?;
    statements
        .iter()
        .map(translate::statement_to_expression)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::parse_program;

    #[test]
    fn parses_a_multi_statement_program() {
        let program = "
            SET rpc_eth = 'https://my-node:8545';
            SELECT nonce, balance FROM accounts
            WHERE address = vitalik.eth AND chain = eth;
            COPY (
              SELECT * FROM logs
              WHERE address = 0xdAC17F958D2ee523a2206206994597C13D831ec7
                AND block_number BETWEEN 4638657 AND 4638758
                AND chain = eth
            ) TO 'usdt.parquet';
        ";
        let expressions = parse_program(program).unwrap();
        assert_eq!(expressions.len(), 3);
    }

    #[test]
    fn legacy_get_reports_equivalent() {
        let err = parse_program("GET balance FROM account vitalik.eth ON eth")
            .unwrap_err()
            .to_string();
        assert!(err.contains("SELECT balance FROM accounts"), "{err}");
    }

    #[test]
    fn join_fails_with_domain_error() {
        let err = parse_program(
            "SELECT * FROM accounts JOIN blocks ON true WHERE address = ian.eth AND chain = eth",
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("JOIN"), "{err}");
    }

    #[test]
    fn example_files_parse() {
        for file in
            std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples")).unwrap()
        {
            let path = file.unwrap().path();
            if path.extension().and_then(|e| e.to_str()) == Some("eql") {
                let source = std::fs::read_to_string(&path).unwrap();
                parse_program(&source).unwrap_or_else(|e| panic!("{path:?}: {e}"));
            }
        }
    }
}
