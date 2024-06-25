# EVM Query Language

This query language allows users to interact with EVM chain data and perform various operations on entities like accounts, blocks, and transactions. Below is a summary of the entities and expressions available in this language:

## Entities:
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
- reward

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

## Expressions:
### GET:
*Description*: Read one or more fields from an entity, given an entity and an id
*Production*: `GET <[fields, ]> FROM <entity> <entity_id> ON <chain>`
*Example*: `GET nonce, balance FROM account vitalik.eth ON base`

### SEND:
*Description*: Sends a transaction to the network
*Production*:  `SEND <type> to=<address>, value=<ether>, data=<bytes> ON <chain>`
*Example*: `SEND TX to=vitalik.eth, value=1, data=0x0...000 ON arbitrum`
           `SEND TOKEN token=0x00...000, to=vitalik.eth amount=0.001 ON ethereum`

### MATH:
*Description*: Supports basic math operations like SUM, SUB, DIV, TIMES
*Production*: `<operator>(<[expr, ]>)`
*Example*: `SUM(GET balance FROM vitalik.eth ON base, GET balance FROM vitalik.eth ON ethereum)`
