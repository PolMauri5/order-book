use rust_decimal::Decimal;


/// Trade does not modify anything, it just descrines what happened.
pub struct Trade {
    pub trade_id: u64,
    pub buy_order_id: u64,
    pub sell_order_id: u64,
    // Precio ejecutado
    pub price: Decimal,
    // Cantidad ejecutada
    pub quantity: Decimal,
    // Posicion determinista
    pub sequence: u64,
}