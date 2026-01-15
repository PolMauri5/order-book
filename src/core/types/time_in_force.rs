#[derive(Clone, Copy)]
pub enum TimeInForce {
    GTC, // Good-Till-Canceled (default)
    IOC, // Immediate-Or-Cancelled (no la entiendo)
    FOK, // Fill-Or-Kill
}