use std::collections::VecDeque;

use rust_decimal::Decimal;

use crate::core::{
    book::price_level::PriceLevel,
    state::{engine_state::EngineState, live_order::LiveOrder},
    types::{event::Event, side::Side, time_in_force::TimeInForce, trade::Trade},
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

    /// Una LIMIT es marketable si al llegar cruza el best del lado contrario.
    fn is_marketable_limit(&self, side: Side, limit_price: Decimal) -> bool {
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

    /// Inserta una orden como resting en el price level (FIFO).
    fn add_resting_to_book(&mut self, side: Side, price: Decimal, order_id: u64, qty: Decimal) {
        let book_side = match side {
            Side::Bid => &mut self.order_book.bids,
            Side::Ask => &mut self.order_book.asks,
        };

        let level = book_side.entry(price).or_insert_with(|| PriceLevel {
            order_ids: VecDeque::new(),
            total_quantity: Decimal::ZERO,
        });

        level.total_quantity += qty;
        level.order_ids.push_back(order_id);
    }

    /// Quita un resting del level y ajusta quantity.
    fn remove_resting_order_from_level(
        &mut self,
        side: Side,
        price: Decimal,
        order_id: u64,
        remaining_qty: Decimal,
    ) {
        let book_side = match side {
            Side::Ask => &mut self.order_book.asks,
            Side::Bid => &mut self.order_book.bids,
        };

        if let Some(level) = book_side.get_mut(&price) {
            // O(n): eliminar el ID del FIFO
            let before = level.order_ids.len();
            level.order_ids.retain(|&id| id != order_id);

            if level.order_ids.len() != before {
                level.total_quantity -= remaining_qty;

                // Seguridad: evitar negativos por desync
                if level.total_quantity < Decimal::ZERO {
                    level.total_quantity = Decimal::ZERO;
                }
            }

            if level.order_ids.is_empty() {
                book_side.remove(&price);
            }
        }
    }

    fn remove_from_active_queue(&mut self, order_id: u64) -> bool {
        let before = self.active_order.len();
        // Elimina solo el order_id
        self.active_order.retain(|&id| id != order_id);
        // Si no tiene la misma len es true
        self.active_order.len() != before
    }

    fn has_sufficient_liquidity(&self, side: Side, limit_price: Decimal, needed: Decimal) -> bool {
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
