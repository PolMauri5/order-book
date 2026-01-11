use rust_decimal::Decimal;

use crate::core::types::order::Order;

pub struct LiveOrder {
    // Order original (inmutable)
    pub order: Order,

    // Cantidad restante para ejecutar
    pub remaining_quantity: Decimal,

    // Indica si la orden sigue activa en el liro
    pub is_active: bool,
}