use std::collections::VecDeque;

#[derive(Clone, Debug)]
pub struct HysteresisBuffer {
    history_size: usize,
    on_ratio: f32,
    on_min_samples: usize,
    off_ratio: f32,
    off_min_samples: usize,
    history: VecDeque<bool>,
    is_active: bool,
    pub is_leaving: bool,
    pub confidence: f32,
    pub hit_count: usize,
    pub sample_count: usize,
    pub ratio: f32,
}

impl HysteresisBuffer {
    pub fn new(
        history_size: usize,
        on_ratio: f32,
        on_min_samples: usize,
        off_ratio: f32,
        off_min_samples: usize,
    ) -> Self {
        Self {
            history_size: history_size.max(1),
            on_ratio,
            on_min_samples: on_min_samples.max(1),
            off_ratio,
            off_min_samples: off_min_samples.max(1),
            history: VecDeque::new(),
            is_active: false,
            is_leaving: false,
            confidence: 0.0,
            hit_count: 0,
            sample_count: 0,
            ratio: 0.0,
        }
    }

    pub fn update(&mut self, is_hit: bool) -> (bool, bool, f32) {
        self.push(is_hit);
        self.update_counts();
        self.update_active_state();
        self.update_leaving_state();
        self.confidence = self.ratio * if self.is_leaving { 0.5 } else { 1.0 };
        (self.is_active, self.is_leaving, self.confidence)
    }

    fn push(&mut self, is_hit: bool) {
        if self.history.len() == self.history_size {
            self.history.pop_front();
        }
        self.history.push_back(is_hit);
    }

    fn update_counts(&mut self) {
        self.sample_count = self.history.len();
        self.hit_count = self.history.iter().filter(|hit| **hit).count();
        self.ratio = if self.sample_count == 0 {
            0.0
        } else {
            self.hit_count as f32 / self.sample_count as f32
        };
    }

    fn update_active_state(&mut self) {
        if self.is_active {
            let should_turn_off =
                self.sample_count >= self.off_min_samples && self.ratio <= self.off_ratio;
            self.is_active = !should_turn_off;
            return;
        }
        self.is_active = self.sample_count >= self.on_min_samples && self.ratio >= self.on_ratio;
    }

    fn update_leaving_state(&mut self) {
        self.is_leaving = false;
        if !self.is_active || self.sample_count < 4 {
            return;
        }

        let half = self.sample_count / 2;
        let first_hits = self.history.iter().take(half).filter(|hit| **hit).count();
        let second_hits = self.history.iter().skip(half).filter(|hit| **hit).count();
        let first_ratio = first_hits as f32 / half as f32;
        let second_ratio = second_hits as f32 / (self.sample_count - half) as f32;
        self.is_leaving = second_ratio < first_ratio;
    }
}

#[cfg(test)]
mod tests {
    use super::HysteresisBuffer;

    #[test]
    fn activates_after_enough_hits() {
        let mut buffer = HysteresisBuffer::new(7, 0.6, 3, 0.35, 7);
        assert!(!buffer.update(true).0);
        assert!(!buffer.update(true).0);
        assert!(buffer.update(false).0);
    }

    #[test]
    fn detects_leaving_when_recent_hits_drop() {
        let mut buffer = HysteresisBuffer::new(6, 0.5, 3, 0.1, 6);
        for hit in [true, true, true, true, false, false] {
            buffer.update(hit);
        }
        assert!(buffer.is_leaving);
        assert_eq!(buffer.confidence, buffer.ratio * 0.5);
    }
}
