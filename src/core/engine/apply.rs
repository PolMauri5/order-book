use rust_decimal::Decimal;

use crate::core::{
    state::{engine_state::EngineState, live_order::LiveOrder},
    types::{event::Event, time_in_force::TimeInForce},
};

impl EngineState {
    /// Aplica un evento de entrada (New / Cancel...) y deja el estado coherente.
    /// Importante: aquí decidimos si una LIMIT entra resting o entra como agresora (marketable).
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

                let order_id = order.order_id;

                // Registrar orden viva (fuente de verdad del remaining/is_active)
                let live_order = LiveOrder {
                    remaining_quantity: order.quantity,
                    is_active: true,
                    order,
                };

                let side = live_order.order.side;
                let price_opt = live_order.order.price;
                let tif = live_order.order.time_in_force;

                // Guardamos independientemente del tipo de orden en live_orders
                self.live_orders.insert(order_id, live_order);

                match price_opt {
                    // MARKET: siempre entra como agresora (nunca resting)
                    None => {
                        // IOC implicitamente
                        self.active_order.push_back(order_id);
                        out_events.push(Event::OrderAccepted { order_id });
                    }

                    // LIMIT: si cruza el spread al llegar -> entra como agresora
                    //        si NO cruza -> depende del TIF (GTC resting, IOC cancel, FOK reject)
                    Some(limit_price) => {
                        if self.is_marketable_limit(side, limit_price) {
                            match tif {
                                // FOK marketable: precheck de liquidez (fill completo o nada)
                                TimeInForce::FOK => {
                                    let qty = self.live_orders[&order_id].remaining_quantity;

                                    // Importante: has_sufficient_liquidity asume book coherente.
                                    // Si total_quantity está sucio, FOK podría pasar y luego no llenarse.
                                    if !self.has_sufficient_liquidity(side, limit_price, qty) {
                                        // FOK falla, nada entra al matching
                                        if let Some(live) = self.live_orders.get_mut(&order_id) {
                                            live.is_active = false;
                                            live.remaining_quantity = Decimal::ZERO;
                                        }

                                        out_events.push(Event::OrderRejected {
                                            order_id,
                                            reason: "FOK: insufficient liquidity".to_string(),
                                        });
                                    } else {
                                        // FOK pasa, entra como agresora
                                        self.active_order.push_back(order_id);
                                        out_events.push(Event::OrderAccepted { order_id });
                                    }
                                }

                                // IOC/GTC marketable: entra como agresora
                                TimeInForce::IOC | TimeInForce::GTC => {
                                    self.active_order.push_back(order_id);
                                    out_events.push(Event::OrderAccepted { order_id });
                                }
                            }
                        } else {
                            // No marketeable:
                            match tif {
                                TimeInForce::GTC => {
                                    // Limit GTC no marketable -> resting
                                    let remaining = self.live_orders[&order_id].remaining_quantity;
                                    self.add_resting_to_book(side, limit_price, order_id, remaining);
                                    out_events.push(Event::OrderAccepted { order_id });
                                }
                                TimeInForce::IOC => {
                                    // Limit IOC no marketable -> cancelar inmediatamente
                                    if let Some(live) = self.live_orders.get_mut(&order_id) {
                                        live.is_active = false;
                                        live.remaining_quantity = Decimal::ZERO;
                                    }
                                    out_events.push(Event::OrderCanceled { order_id });
                                }
                                TimeInForce::FOK => {
                                    // Limit FOK no marketable -> reject inmediato (no entra al matching)
                                    if let Some(live) = self.live_orders.get_mut(&order_id) {
                                        live.is_active = false;
                                        live.remaining_quantity = Decimal::ZERO;
                                    }
                                    out_events.push(Event::OrderRejected {
                                        order_id,
                                        reason: "FOK: not marketable".to_string(),
                                    });
                                }
                            }
                        }
                    }
                }
            }

            // Cancel Order
            Event::CancelOrder { order_id } => {
                let (price, side, remaining_qty) = {
                    let Some(live) = self.live_orders.get(&order_id) else {
                        out_events.push(Event::OrderRejected {
                            order_id,
                            reason: "Unknown order_id".to_string(),
                        });
                        return out_events;
                    };

                    if !live.is_active || live.remaining_quantity.is_zero() {
                        out_events.push(Event::OrderRejected {
                            order_id,
                            reason: "Order already inactive or filled".to_string(),
                        });
                        return out_events;
                    }

                    (live.order.price, live.order.side, live.remaining_quantity)
                };

                // Si estaba en cola agresora lo quitamos
                self.remove_from_active_queue(order_id);

                // Si era limit resting, la quiamos del libro
                if let Some(price) = price {
                    self.remove_resting_order_from_level(side, price, order_id, remaining_qty);
                }

                // Fuente de verdad, marcar como inactiva
                if let Some(live) = self.live_orders.get_mut(&order_id) {
                    live.is_active = false;
                    live.remaining_quantity = Decimal::ZERO;
                }

                out_events.push(Event::OrderCanceled { order_id });
            }

            Event::ModifyOrder { order_id, new_quantity, new_price } => {
                // Validar que la orden exista y esye activa
                let (old_price, side, remaining_qty) = {
                    let Some(live) = self.live_orders.get(&order_id) else {
                        out_events.push(Event::OrderRejected { 
                            order_id,
                            reason: "Unknown order_id".to_string(),
                        });
                        return  out_events;
                    };

                    if !live.is_active || live.remaining_quantity.is_zero() {
                        out_events.push(Event::OrderRejected { 
                            order_id,
                            reason: "Not active or filled".to_string(),
                        });
                        return out_events;
                    }

                    (live.order.price, live.order.side, live.remaining_quantity)
                };

                // Quitar la orden de cualquier sitio en el que pueda estar
                if let Some(price) = old_price {
                    self.remove_resting_order_from_level(side, price, order_id, remaining_qty);
                }
                self.remove_from_active_queue(order_id);

                // Actualizar los campos de la orden
                {
                    let live = self.live_orders.get_mut(&order_id).unwrap();

                    // Nueva cantidad
                    live.remaining_quantity = new_quantity;

                    // Nuevo precio
                    live.order.price = new_price;
                }

                // Reinyectamos la orden como si fuese nueva
                let live = self.live_orders.get(&order_id).unwrap();
                let side = live.order.side;

                match live.order.price {
                    None => {
                        self.active_order.push_back(order_id);
                    }
                    Some(price) => {
                        if self.is_marketable_limit(side, price) {
                            self.active_order.push_back(order_id);
                        } else {
                            // Resting
                            let remaining = live.remaining_quantity;
                            self.add_resting_to_book(side, price, order_id, remaining);
                        }
                    }
                }
                out_events.push(Event::OrderAccepted { order_id });
            }
            _ => {}
        }

        out_events
    }
}