// Events removed - using direct resource-based state management instead.
// HitType is kept here as it's used by the impact handling logic.

#[derive(Debug, Clone, Copy)]
pub enum HitType {
    Planet,
    Blackhole,
    Ship(u8),
}
