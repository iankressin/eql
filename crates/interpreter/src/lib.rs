pub mod backend;
pub mod frontend;

use backend::execution_engine::{ExecutionEngine, QueryResult};
use eql_common::types::Expression;
use frontend::{parser::Parser, sementic_analyzer::SemanticAnalyzer};
use std::error::Error;

pub struct Interpreter;

impl Interpreter {
    pub async fn run_program(source: &str) -> Result<Vec<QueryResult>, Box<dyn Error>> {
        let exressions = Interpreter::run_frontend(source)?;
        Interpreter::run_backend(exressions).await
    }

    fn run_frontend(source: &str) -> Result<Vec<Expression>, Box<dyn Error>> {
        let expressions = Parser::new(source).parse_expressions()?;
        let analyzer = SemanticAnalyzer::new(&expressions);

        analyzer.analyze()?;

        Ok(expressions)
    }

    async fn run_backend(expressions: Vec<Expression>) -> Result<Vec<QueryResult>, Box<dyn Error>> {
        let result = ExecutionEngine::new().run(expressions).await?;

        Ok(result)
    }
}
