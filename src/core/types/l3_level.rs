use rust_decimal::Decimal;

#[allow(dead_code)]
pub struct L3Order {
    pub order_id: u64,
    pub remaining_quantity: Decimal,
}

#[allow(dead_code)]
pub struct L3Level {
    // [price, (order_id, remaining_qty), total_qty]
    pub price: Decimal, // Precio en que se ejecutarian las ordenes de este
    pub orders: Vec<L3Order>, // FIFO (front -> back)
    pub total_quantity: Decimal,
}

#[allow(dead_code)]
pub struct L3Snapshot {
    pub bids: Vec<L3Level>,
    pub asks: Vec<L3Level>,
}