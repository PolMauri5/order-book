use rust_decimal::Decimal;

#[allow(dead_code)]
pub struct L2Level {
    pub price: Decimal,
    pub total_quantity: Decimal
}

#[allow(dead_code)]
pub struct L2Snapshot {
    pub bids: Vec<L2Level>, // Best -> Worst
    pub asks: Vec<L2Level>  // Best -> Worst
}