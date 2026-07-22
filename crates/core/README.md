# EQL Core

EVM Query Language core components.

## Installation
```toml
[dependencies]
eql_core = "0.1"
```

## Usage
EQL queries can be excuted using the `eql` funtion:
```rust
use eql_core::interpreter::eql;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let query = "SELECT balance FROM accounts WHERE address = vitalik.eth AND chain = eth";
    let mut result = eql(query).await?;
    println!("{:?}", result);
    Ok(())
}
```

Or by using `EQLBuilder`:
```rust
use eql_core::common::EQLBuilder;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let result = EQLBuilder::new()
        .get(vec![Field::Account(super::types::AccountField::Balance)])
        .from(
            Entity::Account,
            EntityId::Account(NameOrAddress::Name("vitalik.eth".to_string())),
        )
        .on(Chain::Ethereum)
        .run()
        .await?;

    println!("{:?}", result);
}
````
