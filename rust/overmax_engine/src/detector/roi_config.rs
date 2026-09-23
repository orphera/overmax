use overmax_core::SceneType;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawRoiRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone)]
pub struct SceneRoiConfig {
    pub rois: HashMap<String, RawRoiRect>,
}

#[derive(Debug, Clone)]
pub struct GlobalRoiConfig {
    pub logo: RawRoiRect,
    pub scenes: HashMap<SceneType, SceneRoiConfig>,
}

macro_rules! scenes {
    ( $( $scene:expr => { $( $name:expr => ($x:expr, $y:expr, $w:expr, $h:expr) ),* $(,)? } ),* $(,)? ) => {{
        let mut map = HashMap::new();
        $(
            let mut rois = HashMap::new();
            $(
                rois.insert($name.to_string(), RawRoiRect { x: $x, y: $y, width: $w, height: $h });
            )*
            map.insert($scene, SceneRoiConfig { rois });
        )*
        map
    }};
}

impl Default for GlobalRoiConfig {
    fn default() -> Self {
        let mut scenes = scenes![
            SceneType::Freestyle => {
                "jacket" => (710, 533, 60, 60),
                "rate" => (172, 583, 104, 22),
                "score" => (173, 558, 104, 24),
                "btn_mode" => (80, 130, 5, 5),
                "max_combo_badge" => (409, 585, 36, 36),
                "diff_panel" => (98, 488, 110, 28),
                "gp_center_left" => (702, 80, 7, 257),
                "gp_center_right" => (1211, 80, 7, 257),
                "gp_left_left" => (102, 80, 7, 257),
                "gp_left_right" => (611, 80, 7, 257),
                "gp_right_left" => (1342, 80, 7, 257),
                "gp_right_right" => (1851, 80, 7, 257),
                "pause_title" => (731, 177, 148, 28),
            },
            SceneType::OpenMatch => {
                "jacket" => (664, 533, 60, 60),
                "rate" => (197, 558, 103, 20),
                "score" => (77, 558, 106, 20),
                "btn_mode" => (60, 130, 5, 5),
                "max_combo_badge" => (398, 601, 36, 36),
                "diff_panel" => (82, 467, 116, 31),
            },
            SceneType::ResultFreestyle => {
                "jacket" => (705, 14, 60, 60),
                "rate" => (891, 608, 129, 32),
                "mode" => (0, 18, 340, 75),
                "mode_digit" => (78, 28, 50, 68),
                "mode_colorbar" => (60, 0, 6, 96),
                "diff_panel" => (709, 86, 90, 18),
                "max_combo_badge" => (1024, 521, 75, 75),
                "score" => (759, 710, 407, 94),
            },
            SceneType::ResultOpen3 => {
                "jacket" => (705, 14, 60, 60),
                "rate" => (293, 673, 107, 30),
                "openmatch_mode" => (212, 830, 5, 5),
                "openmatch_diff" => (410, 841, 106, 18),
                "max_combo_badge" => (437, 591, 75, 75),
                "score" => (211, 753, 317, 74),
                "player_panel" => (212, 830, 316, 40),
            },
            SceneType::ResultOpen2 => {
                "jacket" => (705, 14, 60, 60),
                "rate" => (403, 673, 107, 31),
                "openmatch_mode" => (312, 830, 5, 5),
                "openmatch_diff" => (510, 841, 106, 18),
                "max_combo_badge" => (537, 591, 75, 75),
                "score" => (311, 753, 320, 72),
                "player_panel" => (312, 830, 316, 40),
            },
        ];

        // LadderMatch ROI shares same config as OpenMatch
        if let Some(open_match_config) = scenes.get(&SceneType::OpenMatch).cloned() {
            scenes.insert(SceneType::LadderMatch, open_match_config);
        }

        // Online ROI (Lobby menu, empty ROI config)
        scenes.insert(
            SceneType::Online,
            SceneRoiConfig {
                rois: HashMap::new(),
            },
        );

        Self {
            logo: RawRoiRect {
                x: 10,
                y: 10,
                width: 100,
                height: 100,
            },
            scenes,
        }
    }
}
