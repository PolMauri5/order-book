use rust_decimal::Decimal;

use crate::core::types::{order::Order, trade::Trade};

pub enum Event {

    // Eventos de entrada
    NewOrder(Order),

    CancelOrder {
        order_id: u64,
    },

    ModifyOrder {
        order_id: u64,
        new_quantity: Decimal,
        new_price: Option<Decimal>,
    },

    // Eventos de salida

    // Orden aceptada y registrada en el libro
    OrderAccepted {
        order_id: u64,
    },

    // Order Rejected
    OrderRejected {
        order_id: u64,
        reason: String,
    },

    // Order parcialmente ejecutada
    OrderPartiallyFilled {
        order_id: u64,
        filled_quantity: Decimal,
    },

    // Orden completamente ejecutada
    OrderFilled {
        order_id: u64,
    },

    // Orden cancelada exitosamente
    OrderCanceled {
        order_id: u64,
    },

    // Trade generado entre dos órdenes
    Trade(Trade),
}