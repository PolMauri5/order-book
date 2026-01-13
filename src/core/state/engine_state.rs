use crate::core::{book::order_book::OrderBook, state::live_order::LiveOrder, types::order::Order};
use std::collections::HashMap;

/// Esto es el estado mutable del core.
/// Cada evento que entra, lee el estado, lo muta, produce eventos, deja un nuevo estado
pub struct EngineState {
    // Order Book with price levels
    pub order_book: OrderBook,

    // Lookup de ordenes vivas por order_id
    // Estado vivo: cantidad restante, ubicacion, etc.
    pub live_orders: HashMap<u64, LiveOrder>,

    // Secuencia determinista para trades
    pub next_trade: u64,

    // Secuencia determinista global del core
    pub next_sequence: u64,
}

impl EngineState {
    pub fn new() -> Self {
        Self {
            order_book: OrderBook::new(),
            live_orders: HashMap::new(),
            next_trade: 0,
            next_sequence: 0,
        }
    }
}