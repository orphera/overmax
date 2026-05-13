use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub struct GameSessionState {
    pub song_id: Option<u32>,
    pub mode: Option<String>,
    pub diff: Option<String>,
    pub is_stable: bool,
    pub is_max_combo: bool,
    pub rate: Option<f32>,
}

impl GameSessionState {
    pub fn detecting() -> Self {
        Self {
            song_id: None,
            mode: None,
            diff: None,
            is_stable: false,
            is_max_combo: false,
            rate: None,
        }
    }

    pub fn is_valid(&self) -> bool {
        self.song_id.is_some()
            && self.mode.as_ref().is_some_and(|value| !value.is_empty())
            && self.diff.as_ref().is_some_and(|value| !value.is_empty())
            && self.is_stable
    }

    pub fn should_store_rate(&self) -> bool {
        self.rate.is_some_and(|rate| rate > 0.0)
    }
}

impl Default for GameSessionState {
    fn default() -> Self {
        Self::detecting()
    }
}

impl fmt::Display for GameSessionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let status = if self.is_stable { "STABLE" } else { "DETECTING" };
        let mc_status = if self.is_max_combo { " (MAX COMBO)" } else { "" };
        write!(
            f,
            "[{status}] {:?} | {:?} | {:?}{mc_status}",
            self.song_id, self.mode, self.diff
        )
    }
}

#[cfg(test)]
mod tests {
    use super::GameSessionState;

    #[test]
    fn song_id_zero_is_valid_when_state_is_stable() {
        let state = GameSessionState {
            song_id: Some(0),
            mode: Some("4B".to_string()),
            diff: Some("MX".to_string()),
            is_stable: true,
            is_max_combo: false,
            rate: None,
        };

        assert!(state.is_valid());
    }

    #[test]
    fn unstable_state_is_not_valid() {
        let state = GameSessionState {
            song_id: Some(1),
            mode: Some("4B".to_string()),
            diff: Some("MX".to_string()),
            is_stable: false,
            is_max_combo: false,
            rate: Some(99.1),
        };

        assert!(!state.is_valid());
    }

    #[test]
    fn rate_none_and_zero_are_not_stored() {
        let mut state = GameSessionState::detecting();
        assert!(!state.should_store_rate());

        state.rate = Some(0.0);
        assert!(!state.should_store_rate());

        state.rate = Some(1.0);
        assert!(state.should_store_rate());
    }
}
