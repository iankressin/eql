# EVM Query Language

This query language allows users to interact with EVM chain data and perform various operations on entities like accounts, blocks, and transactions. Below is a summary of the entities and expressions available in this language:

## Installation
To begin, install eqlup, the EQL version manager, by running the following command:
```bash
curl https://raw.githubusercontent.com/iankressin/eql/main/eqlup/install.sh | sh
````

Next, install the latest version of EQL using this command:
```bash
eqlup
```

### Updating EQL
To update EQL to the latest version, you can simply run `eqlup` again:
```bash
eqlup
```
## Usage

Queries can be run by executing `.eql` files with `run` command:
```bash
eql run <file>.eql
```

Or using the language REPL:
```bash
eql repl
```

## Expressions:
### GET:
*Description*: Read one or more fields from an entity, given an entity and an id

*Production*: `GET <[fields, ]> FROM <entity> <entity_id> ON <chain>`

*Example*: `GET nonce, balance FROM account vitalik.eth ON base`

### SEND (Soon):
*Description*: Sends a transaction to the network

*Production*:  `SEND <type> to=<address>, value=<ether>, data=<bytes> ON <chain>`

*Example*: `SEND TX to=vitalik.eth, value=1, data=0x0...000 ON arbitrum`

### MATH (Soon):
*Description*: Supports basic math operations like SUM, SUB, DIV, TIMES

*Production*: `<operator>(<[expr, ]>)`

*Example*: `SUM(GET balance FROM vitalik.eth ON base, GET balance FROM vitalik.eth ON ethereum)`

## Entities:

These are the entities that can be queried using the EQL language, each entity is addressed by its name and an id:

### Account:
- address [id]
- nonce
- balance

### Block:
- number [id]
- timestamp
- size 
- hash
- parent hash

### Transaction:
- hash [id]
- from
- to
- data
- value
- fee
- gas price
- timestamp
- status
