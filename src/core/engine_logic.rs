use std::collections::VecDeque;

use rust_decimal::Decimal;

use crate::core::{
    book::price_level::PriceLevel,
    state::{engine_state::EngineState, live_order::LiveOrder},
    types::{event::Event, side::Side, trade::Trade},
};

/// Arquitectura:
/// - EngineState guarda TODO el estado del motor (order_book + live_orders + contadores).
/// - apply_event es el punto de entrada para mutar estado mediante eventos.
///   De momento solo soporta Event::NewOrder.
/// - match_order ejecuta matching "top-of-book":
///   cruza mejor bid vs mejor ask, FIFO dentro del nivel.
/// Nota:
/// - De momento solo gestionamos limit orders con Some(price), no market orders.
impl EngineState {
    /// Mutamos el estado (insert, cancel, modify...)
    /// Devolvemos un evento de salida (accepted, rejected...)
    pub fn apply_event(&mut self, event: Event) -> Vec<Event> {
        let mut out_events = Vec::new();

        match event {
            Event::NewOrder(order) => {
                // Validación mínima
                if order.quantity.is_zero() {
                    out_events.push(Event::OrderRejected {
                        order_id: order.order_id,
                        reason: "Quantity must be > 0".to_string(),
                    });
                    return out_events;
                }

                // Registrar orden viva (fuente de verdad del remaining/is_active)
                let live_order = LiveOrder {
                    remaining_quantity: order.quantity,
                    is_active: true,
                    order,
                };

                let order_id = live_order.order.order_id;
                let price = live_order.order.price;

                // Si es limit (tiene precio), entra al book en su lado y a su nivel FIFO
                // De momento no gestionamos market orders
                if let Some(price) = price {
                    let book_side = match live_order.order.side {
                        Side::Bid => &mut self.order_book.bids,
                        Side::Ask => &mut self.order_book.asks,
                    };

                    // entry(price): crea el nivel si no existe y devuelve una ref mutable al level
                    let level = book_side.entry(price).or_insert_with(|| PriceLevel {
                        order_ids: VecDeque::new(),
                        total_quantity: Decimal::ZERO,
                    });

                    // Cache de volumen del nivel + FIFO por order_id
                    level.total_quantity += live_order.remaining_quantity;
                    level.order_ids.push_back(order_id);
                }

                // Guardamos la orden en el mapa canónico
                self.live_orders.insert(order_id, live_order);

                // Evento de salida: aceptada
                out_events.push(Event::OrderAccepted { order_id });
            }
            // Por ahora ignoramos el resto
            _ => {}
        }

        out_events
    }

    /// Matching básico: cruza mejor bid con mejor ask mientras haya cruce y liquidez.
    pub fn match_order(&mut self) -> Vec<Event> {
        let mut out_events = Vec::new();

        loop {
            // Mejor bid/ask (BTreeMap está ordenado ascendente)
            let bid_price = match self.order_book.bids.keys().next_back().cloned() {
                Some(p) => p,
                None => break,
            };
            let ask_price = match self.order_book.asks.keys().next().cloned() {
                Some(p) => p,
                None => break,
            };

            // Si no hay cruce, terminamos
            if bid_price < ask_price {
                break;
            }

            // Tomar IDs FIFO (sin mantener refs mutables al book)
            let (buy_order_id, sell_order_id) = {
                let bid_level = self.order_book.bids.get(&bid_price).unwrap();
                let ask_level = self.order_book.asks.get(&ask_price).unwrap();

                if bid_level.order_ids.is_empty() || ask_level.order_ids.is_empty() {
                    break;
                }

                (
                    *bid_level.order_ids.front().unwrap(),
                    *ask_level.order_ids.front().unwrap(),
                )
            };

            // Mutar live_orders evitando dos &mut simultáneos al HashMap:
            // sacamos una orden (remove), mutamos la otra (get_mut), y luego reinsertamos.
            let mut buy_order = self.live_orders.remove(&buy_order_id).unwrap();
            let sell_order = self.live_orders.get_mut(&sell_order_id).unwrap();

            // Cantidad ejecutable
            let quantity = buy_order.remaining_quantity.min(sell_order.remaining_quantity);

            // En esta versión simple fijamos el precio al ask del top-of-book
            let price = ask_price;

            // Emitir trade puede ser una ejecucion parcial
            let trade = Trade {
                trade_id: self.next_trade,
                buy_order_id,
                sell_order_id,
                price,
                quantity,
                sequence: self.next_sequence,
            };

            self.next_trade += 1;
            self.next_sequence += 1;

            out_events.push(Event::Trade(trade));

            // Aplicar fills
            buy_order.remaining_quantity -= quantity;
            sell_order.remaining_quantity -= quantity;

            let buy_filled = buy_order.remaining_quantity.is_zero();
            let sell_filled = sell_order.remaining_quantity.is_zero();

            if buy_filled {
                buy_order.is_active = false;
            }
            if sell_filled {
                sell_order.is_active = false;
            }

            // Reinsertar la buy ya actualizada que antes habiamos tomado prestada
            self.live_orders.insert(buy_order_id, buy_order);

            // Actualizar niveles por delta (más barato que recomputar sumas)
            let mut remove_bid_level = false;
            let mut remove_ask_level = false;

            {
                let bid_level = self.order_book.bids.get_mut(&bid_price).unwrap();
                bid_level.total_quantity -= quantity;
                if buy_filled {
                    bid_level.order_ids.pop_front();
                }
                if bid_level.order_ids.is_empty() {
                    remove_bid_level = true;
                }
            }

            {
                let ask_level = self.order_book.asks.get_mut(&ask_price).unwrap();
                ask_level.total_quantity -= quantity;
                if sell_filled {
                    ask_level.order_ids.pop_front();
                }
                if ask_level.order_ids.is_empty() {
                    remove_ask_level = true;
                }
            }

            // Limpiar niveles vacíos (fuera del scope de &mut)
            if remove_bid_level {
                self.order_book.bids.remove(&bid_price);
            }
            if remove_ask_level {
                self.order_book.asks.remove(&ask_price);
            }
        }

        out_events
    }
}
