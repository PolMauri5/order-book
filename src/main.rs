mod adapters;
mod core;

use std::time::Instant;
use rust_decimal::Decimal;

use crate::core::{engine::Engine, state::engine_state::EngineState, types::{event::Event, order::Order, side::Side}};

fn main() {
    let state = EngineState::new();
    let mut engine = Engine::new(state);

    let n = 1_000_000;
    let start = Instant::now();

    for i in 0..n {
        let event = Event::NewOrder(Order {
            order_id: i as u64,
            side: if i % 2 == 0 { Side::Bid } else { Side::Ask },
            price: Some(Decimal::from(100)),
            quantity: Decimal::from(1),
        });

        engine.process(event);
    }

    let elapsed = start.elapsed();
    println!(
        "Processed {} events in {:?} ({:?} per event)",
        n,
        elapsed,
        elapsed / n
    );
}
