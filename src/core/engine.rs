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
        let mut out = Vec::new();

        // Aplicar evento de entrada (new / cancel / modify...)
        // Nos devuelve eventos de salida
        out.extend(self.state.apply_event(event));

        // Ejecutar matching hasta agotarlo
        // Genera Trade events
        out.extend(self.state.match_order());

        out
    }
}
