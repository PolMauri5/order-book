use std::collections::BTreeMap;

use rust_decimal::Decimal;

use crate::core::book::price_level::PriceLevel;

pub struct OrderBook {
    // BTreeMap: mapa ordenado key (price) -> value (price level)
    pub bids: BTreeMap<Decimal, PriceLevel>, // price -> orders, orden descendete
    pub asks: BTreeMap<Decimal, PriceLevel> // price -> orders, orden ascendente
}