mod cli;
mod common;
mod interpreter;
mod repl;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    cli::main().await?;

    Ok(())
}
