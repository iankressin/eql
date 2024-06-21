use pest_derive::Parser as DeriveParser;
use pest::Parser as PestParser;
use std::error::Error;
use super::ast::Field;

#[derive(DeriveParser)]
#[grammar = "src/interpreter/frontend/productions.pest"]
pub struct Parser<'a> {
    source: &'a str
}

impl<'a> Parser<'a> {
    pub fn new(source: &'a str) -> Self {
        Parser { source }
    }

    pub fn build_ast(&self) -> Result<(), Box<dyn Error>> {
        let pairs = Parser::parse(Rule::program, self.source)
            .unwrap_or_else(|e| panic!("{}", e));

        for pair in pairs {
            match pair.as_rule() {
                Rule::get => {
                    let fields = self.build_get_ast(pair.into_inner()); 
                },

                unexpected_token => panic!("Unexpected token: {:?}", unexpected_token),
            }
        }

        Ok(())
    }

    fn build_get_ast(&self, pairs: pest::iterators::Pairs<Rule>) -> Vec<Field> {
        let mut fields = Vec::new();
        let mut current_pair = pairs;

        // TODO: fields has other three types inside. Should add _ in front of the production
        // or keep unwraping the pairs
        while let Some(pair) = current_pair.next() {
            match pair.as_rule() {
                Rule::fields => {
                    // TODO: treat error
                    let field = Field::try_from(pair.as_str()).unwrap();
                    fields.push(field);
                },
                unexpected_token => panic!("Unexpected token: {:?}", unexpected_token),
            }

            current_pair = pair.into_inner();
        }

        println!("{:#?}", fields);

        fields
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_ast() {
        let source = "GET nonce, balance FROM account 0x1234567890123456789012345678901234567890 ON ethereum";
        let parser = Parser::new(source);
        parser.build_ast().unwrap();
    }
}

