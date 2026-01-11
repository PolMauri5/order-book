use crate::core::{state::{engine_state::EngineState, live_order::{self, LiveOrder}}, types::event::{self, Event}};

/// Objetivo: procesar eventos y mutar estado de forma determinista.
impl EngineState {
    pub fn apply_event(&mut self, event: Event) -> Vec<Event> {
        let mut out_events = Vec::new();

        match event {
            Event::NewOrder(order) => {
                // Validaciones mínimas
                if order.quantity.is_zero() {
                    out_events.push(Event::OrderRejected {
                        order_id: order.order_id,
                        reason: "Quantity must be > 0".to_string(),
                    });
                    return out_events;
                }

                // Registrar orden viva
                let live_order = LiveOrder {
                    remaining_quantity: order.quantity,
                    is_active: true,
                    order,
                };

                let order_id = live_order.order.order_id;

                self.live_orders.insert(order_id, live_order.order);

                // (Todavía NO insertamos en el book para matching)
                // Eso viene en el siguiente paso

                out_events.push(Event::OrderAccepted {
                    order_id 
                });
            }
            // Eventos no soportados todavia
            _ => {
                // De momento ignoramos
            }
        }
        out_events
    }   
}