use crate::core::{state::engine_state::EngineState, types::event::Event};

// Orquestrador, no sabe de nada, solo coordina.
pub struct Engine {
    pub state: EngineState,
}

impl Engine {
    pub fn new(state: EngineState) -> Self {
        Self { state }
    }

    // Punto de entrada del motor.
    pub fn process(&mut self, event: Event) -> Vec<Event> {
        let mut out_events = Vec::new();

        // Aplicar evento de entrada (new / cancel / modify...)
        // Nos devuelve eventos de salida: OrderAccepted o OrderRejected
        out_events.extend(self.state.apply_event(event));

        // Ejecutar matching hasta agotarlo
        // Nos devuelve eventos de salida: Trade, OrderFilled, OrderPartiallyFilled
        out_events.extend(self.state.match_order());

        out_events
    }
}
