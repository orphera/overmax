use std::fmt;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SceneType {
    Unknown,
    Freestyle,
    Online,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlayContext {
    pub song_id: u32,
    pub mode: String,
    pub diff: String,
    pub rate: f32,
    pub is_max_combo: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GameSessionState {
    pub context: Option<PlayContext>,
    pub is_stable: bool,
    pub is_fullscreen: bool,
}

impl GameSessionState {
    pub fn detecting() -> Self {
        Self {
            context: None,
            is_stable: false,
            is_fullscreen: false,
        }
    }

    pub fn is_valid(&self) -> bool {
        self.context.is_some() && self.is_stable
    }

    pub fn should_store_rate(&self) -> bool {
        self.context.as_ref().is_some_and(|ctx| ctx.rate > 0.0)
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
        
        match &self.context {
            Some(ctx) => {
                let mc_status = if ctx.is_max_combo { " (MAX COMBO)" } else { "" };
                if ctx.rate > 0.0 {
                    write!(
                        f,
                        "[{status}] {} | {} | {} | {:.2}%{mc_status}",
                        ctx.song_id, ctx.mode, ctx.diff, ctx.rate
                    )
                } else {
                    write!(
                        f,
                        "[{status}] {} | {} | {}{mc_status}",
                        ctx.song_id, ctx.mode, ctx.diff
                    )
                }
            },
            None => write!(f, "[{status}] None | None | None"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{GameSessionState, PlayContext};

    #[test]
    fn song_id_zero_is_valid_when_state_is_stable() {
        let state = GameSessionState {
            context: Some(PlayContext {
                song_id: 0,
                mode: "4B".to_string(),
                diff: "MX".to_string(),
                rate: 0.0,
                is_max_combo: false,
            }),
            is_stable: true,
            is_fullscreen: false,
        };

        assert!(state.is_valid());
    }

    #[test]
    fn unstable_state_is_not_valid() {
        let state = GameSessionState {
            context: Some(PlayContext {
                song_id: 1,
                mode: "4B".to_string(),
                diff: "MX".to_string(),
                rate: 99.1,
                is_max_combo: false,
            }),
            is_stable: false,
            is_fullscreen: false,
        };

        assert!(!state.is_valid());
    }

    #[test]
    fn rate_none_and_zero_are_not_stored() {
        let mut state = GameSessionState::detecting();
        assert!(!state.should_store_rate());

        state.context = Some(PlayContext {
            song_id: 1,
            mode: "4B".to_string(),
            diff: "MX".to_string(),
            rate: 0.0,
            is_max_combo: false,
        });
        assert!(!state.should_store_rate());

        if let Some(ctx) = state.context.as_mut() {
            ctx.rate = 1.0;
        }
        assert!(state.should_store_rate());
    }
}
