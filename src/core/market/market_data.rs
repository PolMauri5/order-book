use rust_decimal::Decimal;

use crate::core::{state::engine_state::EngineState, types::{l2_level::{L2Level, L2Snapshot}, l3_level::{L3Level, L3Order, L3Snapshot}}};

impl EngineState {
    pub fn snapshot_l2(&self, depth: usize) -> L2Snapshot {
        let bids = self.order_book
            .bids
            .iter()
            .rev() // best bid first
            .take(depth) // (Consume como maximo n elementos del iterador y luego se detiene), segun el numero que le pasemos va a coger info hasta n nivel de precios
            .map(|(price, level)| L2Level {
                price: *price,
                total_quantity: level.total_quantity
            })
            .collect();

        let asks = self.order_book
            .asks
            .iter()
            .take(depth)
            .map(|(price, level)| L2Level {
                price: *price,
                total_quantity: level.total_quantity,
            })
            .collect();

        L2Snapshot { bids, asks }
    }

    pub fn snapshot_l3(&self, depth: usize) -> L3Snapshot {
        let bids = self.order_book
            .bids
            .iter()
            .rev()
            .take(depth)
            .map(|(price, level)| {
                let mut total = Decimal::ZERO;
                let orders = level
                    .order_ids
                    .iter()
                    .filter_map(|order_id| {
                        let live = self.live_orders.get(order_id)?;

                        if !live.is_active || live.remaining_quantity.is_zero() {
                            return None;
                        }

                        total += live.remaining_quantity;

                        Some(L3Order {
                            order_id: *order_id,
                            remaining_quantity: live.remaining_quantity,
                        })
                    })
                    .collect::<Vec<_>>();
                L3Level {
                    price: *price,
                    orders,
                    total_quantity: total
                }
            })
            .collect::<Vec<_>>();

        let asks = self.order_book
            .asks
            .iter()
            .take(depth)
            .map(|(price, level)| {
                let mut total = Decimal::ZERO;
                let orders = level
                    .order_ids
                    .iter()
                    .filter_map(|order_id| {
                        let live = self.live_orders.get(order_id)?;

                        if !live.is_active || live.remaining_quantity.is_zero() {
                            return None;
                        }

                        total += live.remaining_quantity;

                        Some(L3Order {
                            order_id: *order_id,
                            remaining_quantity: live.remaining_quantity,
                        })
                    })
                    .collect::<Vec<_>>();

                L3Level {
                    price: *price,
                    orders,
                    total_quantity: total
                }
            })
            .collect::<Vec<_>>();
        L3Snapshot { bids, asks }
    }
}