pub mod backend;
pub mod frontend;

use crate::common::types::Expression;
use backend::execution_engine::{ExecutionEngine, QueryResult};
use frontend::{parser::Parser, sementic_analyzer::SemanticAnalyzer};
use std::error::Error;

pub trait InterpreterResultHandler {
    fn handle_result(&self, result: Vec<QueryResult>);
}

pub struct Interpreter<'a, T>
where
    T: InterpreterResultHandler,
{
    source: &'a str,
    handler: T,
}

impl<T> Interpreter<'_, T>
where
    T: InterpreterResultHandler,
{
    pub fn new(source: &str, handler: T) -> Interpreter<T> {
        Interpreter { source, handler }
    }

    pub async fn run_program(&self) -> Result<(), Box<dyn Error>> {
        let exressions = self.run_frontend()?;
        let result = self.run_backend(exressions).await?;

        self.handler.handle_result(result);

        Ok(())
    }

    pub fn run_frontend(&self) -> Result<Vec<Expression>, Box<dyn Error>> {
        let expressions = Parser::new(self.source).parse_expressions()?;
        let analyzer = SemanticAnalyzer::new(&expressions);

        analyzer.analyze()?;

        Ok(expressions)
    }

    pub async fn run_backend(
        &self,
        expressions: Vec<Expression>,
    ) -> Result<Vec<QueryResult>, Box<dyn Error>> {
        let result = ExecutionEngine::new().run(expressions).await?;

        Ok(result)
    }
}
