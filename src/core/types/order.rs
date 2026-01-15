use rust_decimal::Decimal;

use crate::core::types::{side::Side, time_in_force::TimeInForce};

/// Inmutable, replicable and matching secure.
pub struct Order {
    pub order_id: u64,
    pub side: Side,
    pub price: Option<Decimal>,
    pub quantity: Decimal,
    pub time_in_force: TimeInForce,
}

impl Order {
    pub fn new(
        order_id: u64,
        side: Side,
        price: Option<Decimal>,
        quantity: Decimal,
        time_in_force: TimeInForce,
    ) -> Self {
        let tif = if price.is_none() {
            TimeInForce::IOC
        } else {
            time_in_force
        };

        Self {
            order_id,
            side,
            price,
            quantity,
            time_in_force: tif,
        }
    }
}