# Order Book Engine (Rust)

A deterministic, event-driven order book engine written in Rust.

This project focuses on the **core logic** of an order book: explicit state management, FIFO matching, and a clear processing pipeline.  
There is no networking, persistence, or async logic — the core is fully deterministic and easy to reason about.

## Features

- Event-driven architecture
- Deterministic, single-threaded engine
- FIFO matching at price level (top-of-book)
- Clear separation between orchestration and state
- Performance-oriented design (no locks, no I/O)

## Status

Implemented:
- Limit orders
- Order validation
- Price levels and matching
- Trade generation

Not implemented (yet):
- Market orders
- Cancel / modify
- External integrations

## Run

```bash
cargo run --release
