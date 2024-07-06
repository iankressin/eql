pub mod backend;
pub mod frontend;

use crate::common::types::Expression;
use backend::execution_engine::{ExecutionEngine, QueryResult};
use frontend::{parser::Parser, sementic_analyzer::SemanticAnalyzer};
use std::error::Error;

pub trait InterpreterResultHandler {
    fn handle_result(&self, result: Vec<QueryResult>);
}

pub struct Interpreter<T>
where
    T: InterpreterResultHandler,
{
    handler: T,
}

impl<T> Interpreter<T>
where
    T: InterpreterResultHandler,
{
    pub fn new(handler: T) -> Interpreter<T> {
        Interpreter { handler }
    }

    pub async fn run_program(&self, source: &str) -> Result<(), Box<dyn Error>> {
        let exressions = self.run_frontend(source)?;
        let result = self.run_backend(exressions).await?;

        self.handler.handle_result(result);

        Ok(())
    }

    fn run_frontend(&self, source: &str) -> Result<Vec<Expression>, Box<dyn Error>> {
        let expressions = Parser::new(source).parse_expressions()?;
        let analyzer = SemanticAnalyzer::new(&expressions);

        analyzer.analyze()?;

        Ok(expressions)
    }

    async fn run_backend(
        &self,
        expressions: Vec<Expression>,
    ) -> Result<Vec<QueryResult>, Box<dyn Error>> {
        let result = ExecutionEngine::new().run(expressions).await?;

        Ok(result)
    }
}
