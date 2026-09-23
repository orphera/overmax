use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Mode {
    #[serde(rename = "4B")]
    B4 = 0,
    #[serde(rename = "5B")]
    B5 = 1,
    #[serde(rename = "6B")]
    B6 = 2,
    #[serde(rename = "8B")]
    B8 = 3,
}

impl std::str::FromStr for Mode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "4B" | "4b" => Ok(Self::B4),
            "5B" | "5b" => Ok(Self::B5),
            "6B" | "6b" => Ok(Self::B6),
            "8B" | "8b" => Ok(Self::B8),
            _ => Err(()),
        }
    }
}

impl Mode {
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        <Self as std::str::FromStr>::from_str(s).ok()
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::B4 => "4B",
            Self::B5 => "5B",
            Self::B6 => "6B",
            Self::B8 => "8B",
        }
    }

    pub const ALL: [Self; 4] = [Self::B4, Self::B5, Self::B6, Self::B8];

    pub fn button_count(&self) -> i32 {
        match self {
            Self::B4 => 4,
            Self::B5 => 5,
            Self::B6 => 6,
            Self::B8 => 8,
        }
    }
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Difficulty {
    #[serde(rename = "NM")]
    NM = 0,
    #[serde(rename = "HD")]
    HD = 1,
    #[serde(rename = "MX")]
    MX = 2,
    #[serde(rename = "SC")]
    SC = 3,
}

impl std::str::FromStr for Difficulty {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_uppercase().as_str() {
            "NM" | "NORMAL" => Ok(Self::NM),
            "HD" | "HARD" => Ok(Self::HD),
            "MX" | "MAXIMUM" => Ok(Self::MX),
            "SC" => Ok(Self::SC),
            _ => Err(()),
        }
    }
}

impl Difficulty {
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        <Self as std::str::FromStr>::from_str(s).ok()
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NM => "NM",
            Self::HD => "HD",
            Self::MX => "MX",
            Self::SC => "SC",
        }
    }

    pub fn as_full_name(&self) -> &'static str {
        match self {
            Self::NM => "NORMAL",
            Self::HD => "HARD",
            Self::MX => "MAXIMUM",
            Self::SC => "SC",
        }
    }
}

impl fmt::Display for Difficulty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl Difficulty {
    pub const ALL: [Self; 4] = [Self::NM, Self::HD, Self::MX, Self::SC];

    /// SC 계열 여부
    pub fn is_sc(&self) -> bool {
        matches!(self, Self::SC)
    }
}

pub type RecordKey = (i32, Mode, Difficulty);
pub type RecordValue = (f32, bool);

/// 선곡창/결과창에서 인식된 플레이 기록 데이터
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum PatternRecord {
    /// 기록 없음 / 미플레이 (0.00%)
    #[default]
    Unplayed,
    /// 유효 기록 존재 (80.0% ~ 100.0%)
    Played { rate: f32, is_max_combo: bool },
}

impl PatternRecord {
    #[inline]
    pub fn rate(&self) -> f32 {
        match self {
            Self::Unplayed => 0.0,
            Self::Played { rate, .. } => *rate,
        }
    }

    #[inline]
    pub fn is_max_combo(&self) -> bool {
        match self {
            Self::Unplayed => false,
            Self::Played { is_max_combo, .. } => *is_max_combo,
        }
    }

    #[inline]
    pub fn is_played(&self) -> bool {
        matches!(self, Self::Played { .. })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SceneType {
    Unknown,
    Freestyle,
    Online,
    OpenMatch,
    LadderMatch,
    ResultFreestyle,
    ResultOpen3,
    ResultOpen2,
    Gameplay,
    Paused,
}

impl SceneType {
    /// Scenes that may run song/record recognition and show the normal overlay.
    pub fn is_record_scene(&self) -> bool {
        matches!(self, Self::Freestyle | Self::OpenMatch | Self::LadderMatch) || self.is_result()
    }

    pub fn is_ingame(&self) -> bool {
        matches!(self, Self::Gameplay | Self::Paused)
    }

    #[inline]
    pub fn is_result(&self) -> bool {
        matches!(
            self,
            Self::ResultFreestyle | Self::ResultOpen3 | Self::ResultOpen2
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlayContext {
    pub song_id: i32,
    pub mode: Mode,
    pub diff: Difficulty,
    pub rate: f32,
    pub is_max_combo: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct VerifiedPlayEvent {
    pub song_id: i32,
    pub mode: Mode,
    pub diff: Difficulty,
    pub rate: f32,
    pub is_max_combo: bool,
    pub is_result_screen: bool,
}

impl VerifiedPlayEvent {
    pub fn record_key(&self) -> RecordKey {
        (self.song_id, self.mode, self.diff)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct GameSessionState {
    pub scene: SceneType,
    pub context: Option<PlayContext>,
    pub is_stable: bool,
    pub is_fullscreen: bool,
}

impl GameSessionState {
    pub fn detecting() -> Self {
        Self {
            scene: SceneType::Unknown,
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
        let status = if self.is_stable {
            "STABLE"
        } else {
            "DETECTING"
        };

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
            }
            None => write!(f, "[{status}] None | None | None"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Difficulty, GameSessionState, Mode, PlayContext, SceneType};

    #[test]
    fn song_id_zero_is_valid_when_state_is_stable() {
        let state = GameSessionState {
            scene: SceneType::Freestyle,
            context: Some(PlayContext {
                song_id: 0,
                mode: Mode::B4,
                diff: Difficulty::MX,
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
            scene: SceneType::Freestyle,
            context: Some(PlayContext {
                song_id: 1,
                mode: Mode::B4,
                diff: Difficulty::MX,
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
            mode: Mode::B4,
            diff: Difficulty::MX,
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
