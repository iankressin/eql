pub mod prelex;
pub mod schema;
pub mod values;

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
