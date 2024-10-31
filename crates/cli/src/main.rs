mod repl;

use crate::repl::Repl;
use clap::{Parser, Subcommand};
use csv::ReaderBuilder;
use eql_core::{
    common::query_result::{ExpressionResult, QueryResult},
    interpreter::Interpreter,
};
use serde::Serialize;
use std::error::Error;
use tabled::{builder::Builder, settings::Style, Table};

#[derive(Parser)]
#[clap(
    name = "EQL",
    version = "0.1.0-alpha",
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

    pub fn handle_result(&self, query_results: Vec<QueryResult>) -> Result<(), Box<dyn Error>> {
        for query_result in query_results {
            match query_result.result {
                ExpressionResult::Account(query_res) => {
                    println!("{}", to_table(query_res)?);
                }
                ExpressionResult::Block(query_res) => {
                    println!("{}", to_table(query_res)?);
                }
                ExpressionResult::Transaction(query_res) => {
                    println!("{}", to_table(query_res)?);
                }
                ExpressionResult::Log(query_res) => {
                    println!("{}", to_table(query_res)?);
                }
            }
        }

        Ok(())
    }
}

pub fn to_table<S: Serialize + core::fmt::Debug>(data: Vec<S>) -> Result<Table, Box<dyn Error>> {
    let mut writer = csv::WriterBuilder::new()
        .flexible(true) // Enable flexible mode
        .from_writer(vec![]);

    for entry in data {
        writer.serialize(entry).unwrap();
    }

    let data = String::from_utf8(writer.into_inner().unwrap()).unwrap();
    let mut builder = Builder::default();
    let reader = ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(data.as_bytes());

    for record in reader.into_records() {
        let record = record?;
        let iter = record.iter().map(|s| s.to_owned());
        builder.push_record(iter);
    }

    let mut table = builder.build();
    table.with(Style::rounded());

    Ok(table)
}

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn Error>> {
    let args = Arguments::parse();

    match args.subcmd {
        SubCommand::Run(run_args) => {
            let source = std::fs::read_to_string(run_args.file)?;
            let result_handler = ResultHandler::new();
            let result = Interpreter::run_program(&source).await;
            match result {
                Ok(query_results) => {
                    result_handler.handle_result(query_results)?;
                }
                Err(e) => {
                    eprintln!("{}", e);
                }
            }
        }
        SubCommand::Repl => {
            Repl::new().run().await?;
        }
    }

    Ok(())
}
