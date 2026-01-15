use rust_decimal::Decimal;

use crate::core::{
    state::{engine_state::EngineState},
    types::{side::Side},
};

impl EngineState {
    /// Una LIMIT es marketable si al llegar cruza el best del lado contrario.
    pub fn is_marketable_limit(&self, side: Side, limit_price: Decimal) -> bool {
        match side {
            Side::Bid => {
                // si existe ask y ask <= limit -> cruzable
                self.order_book
                    .asks
                    .keys()
                    .next()
                    .cloned()
                    // Si es None devuelve false
                    .is_some_and(|best_ask| best_ask <= limit_price)
            }
            Side::Ask => {
                // si existe bid y bid >= limit -> cruzable
                self.order_book
                    .bids
                    .keys()
                    .next_back()
                    .cloned()
                    .is_some_and(|best_bid| best_bid >= limit_price)
            }
        }
    }

    pub fn has_sufficient_liquidity(&self, side: Side, limit_price: Decimal, needed: Decimal) -> bool {
        let mut acc = Decimal::ZERO;

        match side {
            Side::Bid => {
                for (price, level) in self.order_book.asks.iter() {
                    if *price > limit_price {
                        break;
                    }
                    acc += level.total_quantity;
                    if acc >= needed {
                        return true;
                    }
                }
            }
            Side::Ask => {
                for (price, level) in self.order_book.bids.iter().rev() {
                    if *price < limit_price {
                        break;
                    }
                    acc += level.total_quantity;
                    if acc >= needed {
                        return true;
                    }
                }
            }
        }

        false
    }
}