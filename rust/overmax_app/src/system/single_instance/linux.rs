pub struct SingleInstanceGuard;

impl SingleInstanceGuard {
    pub fn try_acquire() -> Option<Self> {
        Some(Self)
    }
}
