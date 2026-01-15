# Order Book Engine (Rust)

A deterministic, event-driven **continuous limit order book (CLOB)** engine written in Rust.

This project focuses strictly on the **core matching logic** of an exchange-grade order book:
explicit state transitions, price–time priority, and a deterministic execution pipeline.

There is **no networking, persistence, async runtime, or concurrency**.  
The engine is single-threaded by design to guarantee correctness, reproducibility, and ease of reasoning.

## Design Principles

- Deterministic execution (same input → same output)
- Explicit aggressor / passive order semantics
- Price–time (FIFO) priority at each price level
- Single source of truth for order state
- No implicit behavior or background matching

## Core Concepts

### Aggressor-driven matching only

Trades are generated **only** as a direct consequence of an incoming order:
- Market orders
- Limit orders that are *marketable* on arrival

### Market orders

- Always aggressive
- Consume available liquidity immediately
- May be partially filled
- Remaining quantity is canceled (IOC semantics)
- Never rest in the book

### Limit orders

- Act as aggressive orders on arrival *if they cross the spread*
- Consume liquidity up to their limit price
- Any remaining quantity becomes a resting order
- Non-marketable limits rest immediately

### No resting-vs-resting matching

- Resting orders never execute without an explicit aggressor
- The book does not “self-match”

## Features

- Event-driven architecture
- Deterministic, single-threaded engine
- Aggressor-driven matching (market + marketable limit)
- FIFO matching at each price level
- Explicit order lifecycle management
- Clear separation between orchestration and state
- Performance-oriented design (no locks, no I/O)

## Status

Implemented:
- Market orders (IOC semantics)
- Limit orders (marketable + resting)
- Order cancellation
- Order validation
- Price levels and FIFO matching
- Trade generation
- Explicit terminal events for all orders

Not implemented (yet):
- Order modification
- Time-in-force variants (FOK, GTC, etc.)
- Auctions / batch matching
- Persistence
- Networking / FIX / APIs
- Multi-threading

## Run

```bash
cargo run --release
```