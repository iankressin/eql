mod cli;
mod common;
mod interpreter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    cli::main().await?;

    Ok(())
}
