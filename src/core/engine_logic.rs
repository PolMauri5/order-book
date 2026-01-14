use std::{collections::VecDeque, sync::BarrierWaitResult};
use rust_decimal::Decimal;

use crate::core::{
    book::{order_book, price_level::PriceLevel},
    state::{engine_state::EngineState, live_order::LiveOrder},
    types::{event::Event, side::Side, trade::{self, Trade}},
};

impl EngineState {
    pub fn apply_event(&mut self, event: Event) -> Vec<Event> {
        let mut out_events = Vec::new();

        match event {
            // En caso de nueva orden
            Event::NewOrder(order) => {
                // Validación mínima
                if order.quantity.is_zero() {
                    // "Ejecutamos" el evento de orden rechazada
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

                // Sacamos el order_id para no tener problemas de ownership con order
                let order_id = live_order.order.order_id;

                // Miramos si la orden tiene o no Price (limit (sí) o market (no))
                match live_order.order.price {
                    // Si tiene price:
                    Some(price) => {
                        // Miramos si es una Bid o Ask
                        let book_side = match live_order.order.side {
                            Side::Bid => &mut self.order_book.bids,
                            Side::Ask => &mut self.order_book.asks,
                        };

                        // En caso de que no existe el level, lo creamos
                        let level = book_side.entry(price).or_insert_with(|| PriceLevel {
                            order_ids: VecDeque::new(),
                            total_quantity: Decimal::ZERO,
                        });

                        // Insertamos info en el level en orden
                        level.total_quantity += live_order.remaining_quantity;
                        level.order_ids.push_back(order_id);

                        // Insertamos en el live order, sin orden
                        self.live_orders.insert(order_id, live_order);
                    }
                    None => {
                        // En caso de no tener price, creamos un market_order
                        self.active_order.push_back(live_order);
                    }
                }
                // "Ejecutamos" el evento de orden aceptada 
                out_events.push(Event::OrderAccepted { order_id });
            }
            _ => {}
        }

        out_events
    }

    pub fn match_order(&mut self) -> Vec<Event> {
        let mut out_events = Vec::new();

        // Priorizamos las market orders por orden de llegada que toman liquidez
        while let Some(mut active) = self.active_order.pop_front() {
            loop {
                // Accedemos al price y order id del Live order
                let (price, pasive_order_id) = match active.order.side {
                    // En caso de ser Bid
                    Side::Bid => {
                        // Cogemos el ask_price (que es el price level con el que compararemos)
                        let ask_price = match self.order_book.asks.keys().next().cloned() {
                            Some(p) => p,
                            None => break,
                        };
                        // Cogemos el PriceLevel para acceder a las ordenes
                        let level = self.order_book.asks.get(&ask_price).unwrap();
                        if level.order_ids.is_empty() {
                            break;
                        }
                        // Sacamos la "primera" orden exsitente de ese PriceLevel
                        (ask_price, *level.order_ids.front().unwrap())
                    }
                    // En caso de ask hacemos lo mismo al reves
                    Side::Ask => {
                        let bid_price = match self.order_book.bids.keys().next_back().cloned() {
                            Some(p) => p,
                            None => break,
                        };
                        let level = self.order_book.bids.get(&bid_price).unwrap();
                        if level.order_ids.is_empty() {
                            break;
                        }
                        (bid_price, *level.order_ids.front().unwrap())
                    }
                };

                // Sacamos la live order contra la que atacamos
                let mut pasive = self.live_orders.remove(&pasive_order_id).unwrap();
                // Sacamos el min entre la market order y la pasive
                let qty = active.remaining_quantity.min(pasive.remaining_quantity);

                // Ejecutamos el trade
                let trade = Trade {
                    trade_id: self.next_trade,
                    // El comprador puede ser la orden activa o la pasive
                    // En caso que la orden agresiva sea tipo bid, el comprador have la orden activa
                    // En caso que la orden agresiva sea tipo ask, el comprador hace la pasiva 
                    buy_order_id: if active.order.side == Side::Bid {
                        active.order.order_id
                    } else {
                        pasive.order.order_id
                    },
                    // El vendedor puede ser la orden activa o pasive
                    // En caso que la orden agresiva sea tipo ask, el vendedor hace la orden activa
                    // En caso que la orden agresiva sea tipo bidm el vendedor hace la pasiva
                    sell_order_id: if active.order.side == Side::Ask {
                        active.order.order_id
                    } else {
                        pasive.order.order_id
                    },
                    price,
                    quantity: qty,
                    sequence: self.next_sequence,
                };

                self.next_trade += 1;
                self.next_sequence += 1;

                out_events.push(Event::Trade(trade));

                active.remaining_quantity -= qty;
                pasive.remaining_quantity -= qty;

                // Booleano para ver si la pasiva se ha llenado
                let filled = pasive.remaining_quantity.is_zero();

                // Cogemos todo el PriceLevel del side de la orden pasiva
                let book_side = match pasive.order.side {
                    Side::Bid => &mut self.order_book.bids,
                    Side::Ask => &mut self.order_book.asks,
                };

                let mut remove_level = false;
                {
                    let level = book_side.get_mut(&price).unwrap();
                    level.total_quantity -= qty;
                    if filled {
                        level.order_ids.pop_front();
                    }
                    if level.order_ids.is_empty() {
                        remove_level = true;
                    }
                }

                // Eliminamos tood el nivel ya que no hay mas ordenes en el
                if remove_level {
                    book_side.remove(&price);
                }

                // En caso de que no se ejecute toda la orden la volvemos a insertar
                if !filled {
                    self.live_orders.insert(pasive_order_id, pasive);
                }

                // En caso de que la orden activa se haya completado al 100%, finalizamos
                if active.remaining_quantity.is_zero() {
                    break;
                }
            }

            continue;
        }

        // Resting-resting match (cuando dos limit se consumen entre ellas)
        loop {
            // Sacamos el best bid
            let bid_price = match self.order_book.bids.keys().next_back().cloned() {
                Some(p) => p,
                None => break,
            };
            // Sacamos el best ask
            let ask_price = match self.order_book.asks.keys().next().cloned() {
                Some(p) => p,
                None => break,
            };

            if bid_price < ask_price {
                break;
            }

            // Sacamos el id de las ordenes que vamos a ejecutar
            let (buy_order_id, sell_order_id) = {
                // Conseguimos el PriceLevel correcto
                let bid_level = self.order_book.bids.get(&bid_price).unwrap();
                let ask_level = self.order_book.asks.get(&ask_price).unwrap();
                if bid_level.order_ids.is_empty() || ask_level.order_ids.is_empty() {
                    break;
                }
                (
                    // Buscamos las primeras ordenes de ese PriceLevel
                    *bid_level.order_ids.front().unwrap(),
                    *ask_level.order_ids.front().unwrap(),
                )
            };

            let mut buy_order = self.live_orders.remove(&buy_order_id).unwrap();
            let sell_order = self.live_orders.get_mut(&sell_order_id).unwrap();

            let quantity = buy_order.remaining_quantity.min(sell_order.remaining_quantity);

            let trade = Trade {
                trade_id: self.next_trade,
                buy_order_id,
                sell_order_id,
                price: ask_price,
                quantity,
                sequence: self.next_sequence,
            };

            self.next_trade += 1;
            self.next_sequence += 1;

            out_events.push(Event::Trade(trade));

            buy_order.remaining_quantity -= quantity;
            sell_order.remaining_quantity -= quantity;

            let buy_filled = buy_order.remaining_quantity.is_zero();
            let sell_filled = sell_order.remaining_quantity.is_zero();

            self.live_orders.insert(buy_order_id, buy_order);

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
