use rust_decimal::Decimal;

use crate::core::{
    state::{engine_state::EngineState},
    types::{event::Event, side::Side, time_in_force::TimeInForce, trade::Trade},
};

impl EngineState {
    /// Matching SOLO impulsado por órdenes agresoras (market o limit marketable).
    /// - Market consume hasta donde haya liquidez y el remanente se cancela (IOC implícito).
    /// - Limit marketable consume mientras el precio pasivo cumpla el límite;
    ///   si queda remanente, depende del TIF:
    ///     - GTC -> pasa a resting
    ///     - IOC -> cancela resto
    ///     - FOK -> invariante: no debe quedar remanente (si pasa, bug de book/liquidez)
    pub fn match_order(&mut self) -> Vec<Event> {
        let mut out_events = Vec::new();

        // Procesar agresoras FIFO
        while let Some(active_id) = self.active_order.pop_front() {
            // Si no existe en live_orders (no debería), saltamos
            let Some((active_side, active_price_opt, active_tif)) = self
                .live_orders
                .get(&active_id)
                .map(|o| (o.order.side, o.order.price, o.order.time_in_force))
            else {
                continue;
            };

            // Loop de fills para esa agresora
            loop {
                // Si la activa ya no está activa o está llena, terminamos
                let active_remaining = match self.live_orders.get(&active_id) {
                    Some(live) if live.is_active && !live.remaining_quantity.is_zero() => {
                        live.remaining_quantity
                    }
                    _ => break,
                };

                // Encontrar best pasiva del lado contrario
                let (passive_price, passive_id) = match active_side {
                    Side::Bid => {
                        // Mejor ask
                        let ask_price = match self.order_book.asks.keys().next().cloned() {
                            Some(p) => p,
                            None => break,
                        };

                        // Si la agresora es LIMIT BID, debe cumplir ask_price <= limit_price
                        if let Some(limit_price) = active_price_opt {
                            if ask_price > limit_price {
                                // Ya no puede cruzar más por precio
                                break;
                            }
                        }

                        let level = self.order_book.asks.get(&ask_price).unwrap();
                        let Some(&id) = level.order_ids.front() else { break };
                        (ask_price, id)
                    }

                    Side::Ask => {
                        // Mejor bid
                        let bid_price = match self.order_book.bids.keys().next_back().cloned() {
                            Some(p) => p,
                            None => break,
                        };

                        // Si la agresora es LIMIT ASK, debe cumplir bid_price >= limit_price
                        if let Some(limit_price) = active_price_opt {
                            if bid_price < limit_price {
                                break;
                            }
                        }

                        let level = self.order_book.bids.get(&bid_price).unwrap();
                        let Some(&id) = level.order_ids.front() else { break };
                        (bid_price, id)
                    }
                };

                // Sacamos pasiva del mapa para evitar conflictos de borrow
                let mut passive = match self.live_orders.remove(&passive_id) {
                    Some(p) => p,
                    None => {
                        // Book sucio: id en level que no existe en live_orders
                        // Limpieza mínima: sacamos el front y seguimos
                        //
                        // Importante: ajustamos total_quantity para NO inflar liquidez.
                        // No sabemos el remaining real (porque no existe live), así que la opción segura:
                        // - invalidar el total_quantity (set 0) sería agresivo
                        // - aquí hacemos lo mínimo: recalcular total_quantity del nivel (O(n) del FIFO)
                        //   para mantener coherencia.
                        let book_side = match active_side {
                            Side::Bid => &mut self.order_book.asks,
                            Side::Ask => &mut self.order_book.bids,
                        };

                        if let Some(level) = book_side.get_mut(&passive_price) {
                            level.order_ids.pop_front();

                            // Recalcular total_quantity del level desde live_orders (solo IDs existentes y activas)
                            let mut sum = Decimal::ZERO;
                            for &id in level.order_ids.iter() {
                                if let Some(live) = self.live_orders.get(&id) {
                                    if live.is_active && !live.remaining_quantity.is_zero() {
                                        sum += live.remaining_quantity;
                                    }
                                }
                            }
                            level.total_quantity = sum;

                            if level.order_ids.is_empty() {
                                book_side.remove(&passive_price);
                            }
                        }
                        continue;
                    }
                };

                // Si pasiva está inactiva, limpiamos y seguimos
                if !passive.is_active || passive.remaining_quantity.is_zero() {
                    let book_side = match passive.order.side {
                        Side::Bid => &mut self.order_book.bids,
                        Side::Ask => &mut self.order_book.asks,
                    };

                    if let Some(level) = book_side.get_mut(&passive_price) {
                        if level.order_ids.front().copied() == Some(passive_id) {
                            level.order_ids.pop_front();
                        } else {
                            // Por seguridad: eliminar del FIFO si está en medio (O(n))
                            level.order_ids.retain(|&id| id != passive_id);
                        }

                        // Recalcular total_quantity del level desde live_orders para evitar desync
                        let mut sum = Decimal::ZERO;
                        for &id in level.order_ids.iter() {
                            if let Some(live) = self.live_orders.get(&id) {
                                if live.is_active && !live.remaining_quantity.is_zero() {
                                    sum += live.remaining_quantity;
                                }
                            }
                        }
                        level.total_quantity = sum;

                        if level.order_ids.is_empty() {
                            book_side.remove(&passive_price);
                        }
                    }

                    // no reinsert
                    continue;
                }

                // Qty ejecutada
                let qty = active_remaining.min(passive.remaining_quantity);

                // Price = precio pasivo (best del libro), regla típica para CLOB
                let trade = Trade {
                    trade_id: self.next_trade,
                    buy_order_id: if active_side == Side::Bid { active_id } else { passive_id },
                    sell_order_id: if active_side == Side::Ask { active_id } else { passive_id },
                    price: passive_price,
                    quantity: qty,
                    sequence: self.next_sequence,
                };

                self.next_trade += 1;
                self.next_sequence += 1;

                out_events.push(Event::Trade(trade));

                // Mutar activa
                {
                    let active = self.live_orders.get_mut(&active_id).unwrap();
                    active.remaining_quantity -= qty;

                    if active.remaining_quantity.is_zero() {
                        out_events.push(Event::OrderFilled { order_id: active_id });
                    } else {
                        out_events.push(Event::OrderPartiallyFilled {
                            order_id: active_id,
                            filled_quantity: qty,
                            remaining_quantity: active.remaining_quantity,
                        });
                    }
                }

                // Mutar pasiva (local)
                passive.remaining_quantity -= qty;

                let passive_filled = passive.remaining_quantity.is_zero();
                if passive_filled {
                    out_events.push(Event::OrderFilled { order_id: passive_id });
                    passive.is_active = false;
                } else {
                    out_events.push(Event::OrderPartiallyFilled {
                        order_id: passive_id,
                        filled_quantity: qty,
                        remaining_quantity: passive.remaining_quantity,
                    });
                }

                // Actualizar el level pasivo (siempre front FIFO)
                let book_side = match passive.order.side {
                    Side::Bid => &mut self.order_book.bids,
                    Side::Ask => &mut self.order_book.asks,
                };

                let mut remove_level = false;
                {
                    let level = book_side.get_mut(&passive_price).unwrap();
                    level.total_quantity -= qty;

                    // atacamos al front; si se llenó, lo sacamos del FIFO
                    if passive_filled {
                        if level.order_ids.front().copied() == Some(passive_id) {
                            level.order_ids.pop_front();
                        } else {
                            // Por seguridad (no debería): eliminar del FIFO si está en medio
                            level.order_ids.retain(|&id| id != passive_id);
                        }
                    }

                    // Si por cualquier razón el level queda inconsistente, lo recalculamos
                    // (esto mantiene has_sufficient_liquidity más fiable para FOK)
                    if level.total_quantity < Decimal::ZERO {
                        let mut sum = Decimal::ZERO;
                        for &id in level.order_ids.iter() {
                            if let Some(live) = self.live_orders.get(&id) {
                                if live.is_active && !live.remaining_quantity.is_zero() {
                                    sum += live.remaining_quantity;
                                }
                            }
                        }
                        level.total_quantity = sum;
                    }

                    if level.order_ids.is_empty() {
                        remove_level = true;
                    }
                }

                if remove_level {
                    book_side.remove(&passive_price);
                }

                // Reinsertar pasiva si queda qty
                if !passive_filled {
                    self.live_orders.insert(passive_id, passive);
                }

                // Si la activa se llenó, salimos
                let done = self
                    .live_orders
                    .get(&active_id)
                    .map(|o| o.remaining_quantity.is_zero())
                    .unwrap_or(true);

                if done {
                    break;
                }
            }

            // Post-proceso de la agresora:
            // - Si es MARKET y queda quantity -> se cancela/expira (IOC implícito).
            // - Si es LIMIT y queda quantity -> depende del TIF:
            //     GTC -> pasa a resting
            //     IOC -> cancela resto
            //     FOK -> BUG: no debería quedar remanente si el precheck era correcto
            if let Some(active) = self.live_orders.get_mut(&active_id) {
                if active.is_active && !active.remaining_quantity.is_zero() {
                    match active.order.price {
                        None => {
                            // MARKET: no puede quedarse en libro (resting)
                            // Esto es un comportamiento IOC implícito (market = IOC)
                            out_events.push(Event::OrderCanceled { order_id: active_id });
                            active.is_active = false;
                            active.remaining_quantity = Decimal::ZERO;
                        }
                        Some(limit_price) => match active_tif {
                            TimeInForce::GTC => {
                                // LIMIT GTC: lo que no se ejecuta pasa a resting
                                let side = active.order.side;
                                let remaining = active.remaining_quantity;

                                self.add_resting_to_book(side, limit_price, active_id, remaining);
                            }
                            TimeInForce::IOC => {
                                // LIMIT IOC: lo que no se ejecuta se cancela (no queda resting)
                                out_events.push(Event::OrderCanceled { order_id: active_id });
                                active.is_active = false;
                                active.remaining_quantity = Decimal::ZERO;
                            }
                            TimeInForce::FOK => {
                                // LIMIT FOK: NO debería llegar aquí con remanente.
                                // Si pasa, es bug (precheck mintió o book/cache inconsistente).
                                out_events.push(Event::OrderRejected {
                                    order_id: active_id,
                                    reason: "FOK: remaining quantity after matching (BUG)".to_string(),
                                });
                                active.is_active = false;
                                active.remaining_quantity = Decimal::ZERO;
                            }
                        },
                    }
                }
            }
        }

        // IMPORTANTE:
        // No hay resting-vs-resting match.
        // Si no entra una agresora, no se generan trades.

        out_events
    }
}