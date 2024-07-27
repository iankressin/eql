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
    let mut result = eql("GET balance FROM account vitalik.eth ON eth").await?;
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
