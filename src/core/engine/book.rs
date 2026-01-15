use std::collections::VecDeque;

use rust_decimal::Decimal;

use crate::core::{
    book::price_level::PriceLevel,
    state::{engine_state::EngineState},
    types::{side::Side},
};

impl EngineState {
    /// Inserta una orden como resting en el price level (FIFO).
    pub fn add_resting_to_book(&mut self, side: Side, price: Decimal, order_id: u64, qty: Decimal) {
        let book_side = match side {
            Side::Bid => &mut self.order_book.bids,
            Side::Ask => &mut self.order_book.asks,
        };

        let level = book_side.entry(price).or_insert_with(|| PriceLevel {
            order_ids: VecDeque::new(),
            total_quantity: Decimal::ZERO,
        });

        level.total_quantity += qty;
        level.order_ids.push_back(order_id);
    }

    /// Quita un resting del level y ajusta quantity.
    pub fn remove_resting_order_from_level(
        &mut self,
        side: Side,
        price: Decimal,
        order_id: u64,
        remaining_qty: Decimal,
    ) {
        let book_side = match side {
            Side::Ask => &mut self.order_book.asks,
            Side::Bid => &mut self.order_book.bids,
        };

        if let Some(level) = book_side.get_mut(&price) {
            // O(n): eliminar el ID del FIFO
            let before = level.order_ids.len();
            level.order_ids.retain(|&id| id != order_id);

            if level.order_ids.len() != before {
                level.total_quantity -= remaining_qty;

                // Seguridad: evitar negativos por desync
                if level.total_quantity < Decimal::ZERO {
                    level.total_quantity = Decimal::ZERO;
                }
            }

            if level.order_ids.is_empty() {
                book_side.remove(&price);
            }
        }
    }

    pub fn remove_from_active_queue(&mut self, order_id: u64) -> bool {
        let before = self.active_order.len();
        // Elimina solo el order_id
        self.active_order.retain(|&id| id != order_id);
        // Si no tiene la misma len es true
        self.active_order.len() != before
    }
}