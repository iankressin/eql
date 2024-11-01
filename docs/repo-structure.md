# EQL Repository Structure

This document is subject to change as EQL is actively being improved. If you find any inconsistencies, please raise an issue in the repository. Your contribution is highly valuable.

The `/crates` directory in the `iankressin/eql` repository contains all the Rust code for the project, organized into three main subdirectories: `/cli`, `/core`, `/wasm` and `/macros`. Each subdirectory houses Rust source code for different components of the EVM Query Language (EQL) project.

## `/crates/cli`

This directory contains the source code for the EQL command-line interface (CLI) tool.

- **`main.rs`**: 
  - The entry point for the EQL program. It defines the `Arguments` struct using the `clap` crate for parsing command-line arguments, supporting `run` (to execute a `.eql` file) and `repl` (to start an interactive REPL).
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
  - **`parser.rs`**: Uses the [Pest Parser library](https://pest.rs/) to define syntax rules and match expressions into defined structs. Includes functions like `parse_expressions` (handles different expression types), `parse_get_expr` (parses GET expressions), and `get_fields` (handles fields in GET expressions).
  - **`productions.pest`**: Contains the syntax rules for EQL expressions. It can be tested in the Pest [playground](https://pest.rs/#editor).
  - **`semantic_analyzer.rs`**: Performs additional checks on parsed expressions, ensuring fields correspond to their entities.

- **`backend` module**: 
  - Executes queries using the [Alloy](https://docs.rs/alloy/0.2.0/alloy/index.html) library.
  - **`execution_engine.rs`**: Processes parsed expressions and executes them, with functions tailored to handle different entity types and their respective query requirements.
  - **`resolve_account.rs`**: Handles account queries using `alloy::{get_balance, get_transaction_count, get_code_at}`
  - **`resolve_block.rs`**: Handles block queries using `alloy::get_block_by_number`
  - **`resolve_logs.rs`**: Handles event log queries using `alloy::get_logs`
  - **`resolve_transaction.rs`**: Handles transaction queries using `alloy::get_transaction_by_hash`

### `common` Module

Contains types and utilities shared across the program:

- **`query_builder.rs`**: Defines the `EQLBuilder` struct for constructing and executing EQL queries
- **`types.rs`**: Core types and enums including `Expression`, `Field`, and entity-specific fields
- **`entity.rs`**: Defines the `Entity` enum for blockchain entities (Block, Transaction, Account, Logs)
- **`query_result.rs`**: Defines result schemas for different entity types
- **`chain.rs`**: Defines supported blockchain networks and their RPC configurations
- **`ens.rs`**: Implements ENS resolution functionality
- **`serializer.rs`**: Handles data export to JSON, CSV, and Parquet formats
- **`filters.rs`**: Implements generic filtering traits and types (EqualityFilter, ComparisonFilter) used across entities
- **`account.rs`**: Defines Account entity structure, fields, and filters
- **`block.rs`**: Defines Block entity structure, fields, and filters
- **`transaction.rs`**: Defines Transaction entity structure, fields, and filters
- **`logs.rs`**: Defines Log entity structure, fields, and filters

## `/crates/wasm`

This directory contains WebAssembly bindings for EQL:

- **`lib.rs`**: Exposes the `eql` async function that processes EQL programs and returns results as `JsValue`

## `/crates/macros`

Contains procedural macros for eql_core:

- **`lib.rs`**: Defines the `EnumVariants` macro for generating `all_variants()` methods

## Installation & Configuration

The project includes an installation system:

- **`eqlup/install.sh`**: Installs the EQL version manager
- **`eqlup/eqlup.sh`**: Manages EQL versions and configuration
- **`eql-config.json`**: Contains RPC endpoints for supported networks

## Documentation

The `/docs` directory contains comprehensive documentation:

- **`installation.md`**: Installation and configuration guide
- **`query.md`**: Query syntax and examples
- **`entities.md`**: Entity types and their available fields
- **`advanced.md`**: Advanced usage patterns

## Summary

This document outlines the structure and purpose of each component within the EQL repository. For more detailed and up-to-date information, refer directly to the source code, and contribute improvements or report issues via the repository's issue tracker.
