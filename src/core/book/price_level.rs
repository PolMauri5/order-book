use std::collections::VecDeque;

use rust_decimal::Decimal;

use crate::core::types::order::Order;

pub struct PriceLevel {
    // VecDeque: Double-ended queue
    // FIFO / LIFO: 
    // Añadir al final con push_back
    // Sacar del frente con pop_front
    // Mas eficiente que Vec
    pub orders: VecDeque<Order>, // FIFO Queue
    // Lo necesitamos para matching rapdio, podriamos ver si hay
    // cantidad suficiente para un match real o total si iterar por todos
    // los precios de las ordenes
    pub total_quantity: Decimal, // Suma las cantidades restantes
}