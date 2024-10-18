pub mod backend;
pub mod frontend;

use crate::common::{query_result::QueryResult, types::Expression};
use anyhow::Result;
use backend::execution_engine::ExecutionEngine;
use frontend::parser::Parser;

pub struct Interpreter;

#[derive(Debug, thiserror::Error)]
pub enum InterpreterError {
    #[error(
        "eql() should receive a single query. For multiple queries use Interpreter::run_program"
    )]
    SingleQueryError,
}

impl Interpreter {
    pub async fn run_program(source: &str) -> Result<Vec<QueryResult>> {
        let exressions = Interpreter::run_frontend(source)?;
        Interpreter::run_backend(exressions).await
    }

    fn run_frontend(source: &str) -> Result<Vec<Expression>> {
        let expressions = Parser::new(source).parse_expressions()?;
        Ok(expressions)
    }

    async fn run_backend(expressions: Vec<Expression>) -> Result<Vec<QueryResult>> {
        let result = ExecutionEngine::new().run(expressions).await?;
        Ok(result)
    }
}

pub async fn eql(source: &str) -> Result<QueryResult> {
    let result = Interpreter::run_program(source).await?;

    match result.first() {
        Some(result) => Ok(result.clone()),
        None => Err(InterpreterError::SingleQueryError.into()),
    }
}
