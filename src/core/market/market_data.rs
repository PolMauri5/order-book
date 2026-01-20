use crate::core::{state::engine_state::EngineState, types::l2_level::{L2Level, L2Snapshot}};

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
}