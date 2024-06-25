pub mod frontend;
pub mod backend;

use frontend::{
    parser::Parser,
    sementic_analyzer::SemanticAnalyzer
};


pub struct Interpreter<'a> {
    source: &'a str,
}

impl Interpreter<'_> {
    pub fn new(source: &str) -> Interpreter {
        Interpreter { source }
    }

    pub fn run(&self) -> Result<(), &'static str> {
        let expressions = Parser::new(self.source).parse_expressions()?;
        let analyzer = SemanticAnalyzer::new(&expressions);

        analyzer.analyze()?;

        Ok(())
    }
    
}
