
# EQL Repository Structure

This document is subject to change as EQL is actively being improved. If you find any inconsistencies, please raise an issue in the repository. Your contribution is highly valuable.

The `/crates` directory in the `iankressin/eql` repository contains all the Rust code for the project, organized into three main subdirectories: `/cli`, `/core`, `/wasm` and `/macros`. Each subdirectory houses Rust source code for different components of the EVM Query Language (EQL) project.

## `/crates/cli`

This directory contains the source code for the EQL command-line interface (CLI) tool.

- **`main.rs`**: 
  - The entry point for the EQL program. It defines the `Arguments` struct using the `clap` crate for parsing command-line arguments, supporting `run` (to execute a `.eql` file) and `reply` (to start an interactive REPL).
  - The `ResultHandler` struct manages the display of program or query results in a formatted manner.
  - The `main` function sets up the asynchronous runtime using `tokio`, initializes the `Arguments` struct, and handles either execution of EQL expressions via `Interpreter::run_program` or starts a REPL session.

- **`repl.rs`**: 
  - Implements a Read-Eval-Print Loop (REPL) for the EQL CLI, handling user inputs, executing EQL expressions, and displaying results interactively using the `crossterm` crate for terminal manipulation, `eql_core` for expression interpretation, and `tabled` for result formatting.
  - The `Repl` struct manages REPL session state, including command history, cursor position, current expression, and output display.
  - Key methods include `run` (loops through user's inputs), `redraw_line` (updates the current line in the REPL), `run_expression` (executes expressions using the `Interpreter`), and `display_result` (formats and shows the results).

## `/crates/core`

This directory contains the logic for interpreting and executing EQL expressions, divided into two primary modules: `interpreter` and `common`.

### `interpreter` Module

Contains the code for executing EQL queries, with key responsibilities split between frontend (parsing and analysis) and backend (query execution).

- **`mod.rs`**: 
  - Coordinates the frontend (parsing and analysis) and backend (execution) components.
  - The `Interpreter` struct implements `run_program`, `run_frontend` (parses expressions and performs semantic checks), and `run_backend` (executes parsed expressions).
  - The `eql` function serves as the interface for the wasm module.

- **`frontend` module**: 
  - Parses expression arguments into an `Expression` struct.
  - **`parser.rs`**: Uses the [Pest Parser library](https://pest.rs/) to define syntax rules and match expressions into defined structs. Includes functions like `parse_expressions` (handles different expression types), `parse_get_expr` (parses GET expressions), and `get_fields` (handles fields in GET expressions). Pest includes a [book](https://pest.rs/book/) and this [video](https://www.youtube.com/watch?v=VYBi9an29Hw) provides a simple introduction to it.
  - **`productions.pest`**: Contains the syntax rules for EQL expressions. It can be tested in the Pest [playground](https://pest.rs/#editor).
  - **`SemanticAnalyzer`**: Performs additional checks on parsed expressions, ensuring fields correspond to their entities.

- **`backend` module**: 
  - Executes queries using the [Alloy](https://docs.rs/alloy/0.2.0/alloy/index.html) library.
  - **`execution_engine.rs`**: Processes parsed expressions and executes them, with functions tailored to handle different entity types and their respective query requirements. It implements the `run_get_expr` (for each entity type, it parse the fields into a vector, resolve the entity IDs and Filters, and call the respective resolve function for that entity).
  - **`resolve_account`**: Called in execution engine when `Entity::Account`, map each `account_id` to a future list and concurrently collect the results. Two auxiliary functions are `to_address` (resolve ENS) and `get_account` (query accounts using `alloy::{get_balance, get_transaction_count, get_code_at}`).
  - **`resolve_block`**: Called in execution engine when `Entity::Block`, map each `block_id` to a future list and concurrently collect the results, flattening results into a single vec. Two auxiliary functions are `batch_get_block` (call `get_block` to all BlockRange) and `get_block` (query blocks using `alloy::get_block_by_number`).
  - **`resolve_logs`**: Called in execution engine when `Entity::Log`, it has one function `resolve_log_query` (query logs using `alloy::get_logs` with the provided filter).
  - **`resolve_transaction`**: Called in execution engine when `Entity::Transaction`, map each `transaction_id` to a future list and concurrently collect the results. The auxiliary functions is `get_transaction` (query transactions fields using `alloy::get_transaction_by_hash`).

### `common` Module

Contains types and utilities shared across the program, such as entities, query builders, and results.

- **`query_builder.rs`**: Defines the `EQLBuilder` struct for constructing and executing EQL queries, allowing specification of fields, entities, entity_id, entity_filter, chains and dump.
- **`types.rs`**: Includes various types and enums like `Expression`, `Field`, entity-specific fields, with conversion methods for parsing and `DumpFormat`.
- **`entity.rs`**: Defines the `Entity` enum for blockchain entities (e.g., Block, Transaction, Account and Logs) and methods for string conversion.
- **`entity_id.rs`**: Defines the `EntityId` enum for different entity identifiers, supporting conversions from Pest pairs.
- **`entity_filter.rs`**: Defines the `EntityFilter` enum represents different types of filter you can make when quering. It's mainly used for Logs.It also supports conversions from Pest pairs.
- **`query_result.rs`**: Defines structs for query results corresponding to different entity types, and defining the schema each entity have.
- **`chain.rs`**: Provides the `Chain` enum, representing various blockchains (e.g., Ethereum, Arbitrum) with associated RPC URLs and IDs.
- **`ens.rs`**: Implements Ethereum Name Service (ENS) functionality, including address resolution and the namehash algorithm.
- **`serializer.rs`**: Implements functionality for dumping query results into different file formats. It provides functions to serialize data into JSON, CSV, and Parquet formats. The main function dump_results takes an ExpressionResult and a Dump configuration, then writes the serialized data to a file based on the specified format. 

## `/crates/wasm`

This directory contains a Rust crate providing WebAssembly bindings for EQL.

- **`lib.rs`**: Exposes an asynchronous function `eql` that interprets a string-based EQL program using `eql_interpreter` from `eql_core`, returning results as `JsValue`.
- 
## `/crates/macros`

This directory contains the procedural macros for eql_core.

- **`lib.rs`**: Defines the only procedural macro currently, called EnumVariants (uses the `proc_macro` crate to generate `all_variants()` method for the enum.)


## Other Files

- **`Cargo.toml`**: Lists the dependencies required for the crate.
- **`src` Directory**: Contains the Rust source files for each module.

## Summary

This document outlines the structure and purpose of each component within the EQL repository. For more detailed and up-to-date information, refer directly to the source code, and contribute improvements or report issues via the repository's issue tracker.
