use crate::interpreter::{
    backend::execution_engine::{ExpressionResult, QueryResult},
    Interpreter, InterpreterResultHandler,
};
use clap::{Parser, Subcommand};
use std::error::Error;
use tabled::{settings::Style, Table};

#[derive(Parser)]
#[clap(
    name = "EQL",
    version = "0.1.0-beta",
    author = "Ian K. Guimaraes <ianguimaraes31@gmail.com>"
)]
struct Arguments {
    #[clap(subcommand)]
    subcmd: SubCommand,
}

#[derive(Debug, Subcommand)]
enum SubCommand {
    #[clap(name = "run", about = "Run an .eql file")]
    Run(RunArguments),

    #[clap(name = "repl", about = "Start an interactive REPL")]
    Repl,
}

#[derive(Debug, Parser)]
struct RunArguments {
    file: String,
}

struct ResultHandler;

impl ResultHandler {
    pub fn new() -> Self {
        ResultHandler
    }
}

impl InterpreterResultHandler for ResultHandler {
    // TODO: this can be refactored to be more generic
    fn handle_result(&self, query_results: Vec<QueryResult>) {
        for query_result in query_results {
            match query_result.result {
                ExpressionResult::Account(query_res) => {
                    println!("> {}", query_result.query);
                    let mut table = Table::new(vec![query_res]);
                    table.with(Style::rounded());
                    println!("{}\n", table.to_string());
                }
                ExpressionResult::Block(query_res) => {
                    println!("> {}", query_result.query);
                    let mut table = Table::new(vec![query_res]);
                    table.with(Style::rounded());
                    println!("{}\n", table.to_string());
                }
                ExpressionResult::Transaction(query_res) => {
                    println!("> {}", query_result.query);
                    let mut table = Table::new(vec![query_res]);
                    table.with(Style::rounded());
                    println!("{}\n", table.to_string());
                }
            }
        }
    }
}

pub async fn main() -> Result<(), Box<dyn Error>> {
    let args = Arguments::parse();

    match args.subcmd {
        SubCommand::Run(run_args) => {
            let source = std::fs::read_to_string(run_args.file)?;
            let result_handler = ResultHandler::new();

            Interpreter::new(&source, result_handler)
                .run_program()
                .await?;
        }
        SubCommand::Repl => {
            println!("Starting REPL");
        }
    }

    Ok(())
}
