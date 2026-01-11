use rust_decimal::Decimal;

use crate::core::types::side::Side;

/// Inmutable, replicable and matching secure.
pub struct Order {
    pub order_id: u64,
    pub side: Side,
    // None = Market Order
    pub price: Option<Decimal>,
    pub quantity: Decimal,
    // Deterministic arrival order
    pub arrival_seq: u64,
} 