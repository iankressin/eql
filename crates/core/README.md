# EQL Core

EVM Query Language core components.

## Installation
```toml
[dependencies]
eql_core = "0.1"
```

## Usage
```rust
use eql_core::interpreter::Interpreter;

#[tokio::main]
async fn main() {
    let mut interpreter = Interpreter::run("GET balance FROM account vitalik.eth ON eth");
    println!("{:?}", result);
}
```
