use overmax_core::SceneType;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoiRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneRoiConfig {
    pub rois: HashMap<String, RoiRect>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalRoiConfig {
    pub scenes: HashMap<SceneType, SceneRoiConfig>,
}

impl Default for GlobalRoiConfig {
    fn default() -> Self {
        let mut scenes = HashMap::new();
        
        // Freestyle ROI
        let mut freestyle_rois = HashMap::new();
        freestyle_rois.insert("jacket".to_string(), RoiRect { x: 710, y: 534, width: 58, height: 58 });
        freestyle_rois.insert("rate".to_string(), RoiRect { x: 176, y: 583, width: 94, height: 22 });
        freestyle_rois.insert("btn_mode".to_string(), RoiRect { x: 80, y: 130, width: 5, height: 5 });
        freestyle_rois.insert("max_combo_badge".to_string(), RoiRect { x: 409, y: 587, width: 36, height: 33 });
        freestyle_rois.insert("diff_panel".to_string(), RoiRect { x: 98, y: 488, width: 110, height: 28 });
        scenes.insert(SceneType::Freestyle, SceneRoiConfig { rois: freestyle_rois });

        // OpenMatch ROI
        let mut open_match_rois = HashMap::new();
        open_match_rois.insert("jacket".to_string(), RoiRect { x: 664, y: 534, width: 60, height: 58 });
        open_match_rois.insert("rate".to_string(), RoiRect { x: 191, y: 554, width: 94, height: 27 });
        open_match_rois.insert("btn_mode".to_string(), RoiRect { x: 60, y: 130, width: 5, height: 5 });
        open_match_rois.insert("max_combo_badge".to_string(), RoiRect { x: 397, y: 601, width: 36, height: 36 });
        open_match_rois.insert("diff_panel".to_string(), RoiRect { x: 82, y: 467, width: 116, height: 31 });
        scenes.insert(SceneType::OpenMatch, SceneRoiConfig { rois: open_match_rois.clone() });

        // LadderMatch ROI
        scenes.insert(SceneType::LadderMatch, SceneRoiConfig { rois: open_match_rois });

        // Online ROI (Lobby menu, empty ROI config)
        scenes.insert(SceneType::Online, SceneRoiConfig { rois: HashMap::new() });
        
        // ResultFreestyle ROI
        let mut result_freestyle_rois = HashMap::new();
        result_freestyle_rois.insert("jacket".to_string(), RoiRect { x: 630, y: 10, width: 60, height: 60 });
        result_freestyle_rois.insert("rate".to_string(), RoiRect { x: 430, y: 580, width: 100, height: 30 });
        result_freestyle_rois.insert("mode".to_string(), RoiRect { x: 20, y: 15, width: 320, height: 75 });
        result_freestyle_rois.insert("diff_panel".to_string(), RoiRect { x: 700, y: 75, width: 110, height: 33 });
        result_freestyle_rois.insert("max_combo_badge".to_string(), RoiRect { x: 760, y: 650, width: 200, height: 220 });
        scenes.insert(SceneType::ResultFreestyle, SceneRoiConfig { rois: result_freestyle_rois });

        // ResultOpen3 ROI (오픈매치 3인+)
        let mut result_open3_rois = HashMap::new();
        result_open3_rois.insert("jacket".to_string(), RoiRect { x: 705, y: 15, width: 60, height: 60 });
        result_open3_rois.insert("rate".to_string(), RoiRect { x: 220, y: 640, width: 120, height: 30 });
        result_open3_rois.insert("mode_diff_badge".to_string(), RoiRect { x: 108, y: 765, width: 166, height: 45 });
        result_open3_rois.insert("max_combo_badge".to_string(), RoiRect { x: 200, y: 530, width: 90, height: 80 });
        scenes.insert(SceneType::ResultOpen3, SceneRoiConfig { rois: result_open3_rois });

        // ResultOpen2 ROI (오픈매치 2인)
        let mut result_open2_rois = HashMap::new();
        result_open2_rois.insert("jacket".to_string(), RoiRect { x: 690, y: 15, width: 60, height: 60 });
        result_open2_rois.insert("rate".to_string(), RoiRect { x: 420, y: 640, width: 120, height: 30 });
        result_open2_rois.insert("mode_diff_badge".to_string(), RoiRect { x: 156, y: 800, width: 166, height: 45 });
        result_open2_rois.insert("max_combo_badge".to_string(), RoiRect { x: 250, y: 560, width: 90, height: 80 });
        scenes.insert(SceneType::ResultOpen2, SceneRoiConfig { rois: result_open2_rois });

        Self { scenes }
    }
}
