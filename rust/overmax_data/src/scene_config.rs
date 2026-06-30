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
        result_freestyle_rois.insert("jacket".to_string(), RoiRect { x: 705, y: 14, width: 60, height: 60 });
        result_freestyle_rois.insert("rate".to_string(), RoiRect { x: 891, y: 608, width: 109, height: 32 });
        result_freestyle_rois.insert("mode".to_string(), RoiRect { x: 5, y: 18, width: 335, height: 75 });
        result_freestyle_rois.insert("diff_panel".to_string(), RoiRect { x: 707, y: 84, width: 93, height: 21 });
        result_freestyle_rois.insert("max_combo_badge".to_string(), RoiRect { x: 1027, y: 524, width: 75, height: 73 });
        result_freestyle_rois.insert("score".to_string(), RoiRect { x: 759, y: 710, width: 407, height: 94 });
        scenes.insert(SceneType::ResultFreestyle, SceneRoiConfig { rois: result_freestyle_rois });
 
        // ResultOpen3 ROI (오픈매치 3인+)
        let mut result_open3_rois = HashMap::new();
        result_open3_rois.insert("jacket".to_string(), RoiRect { x: 705, y: 14, width: 60, height: 60 });
        result_open3_rois.insert("rate".to_string(), RoiRect { x: 293, y: 673, width: 107, height: 30 });
        result_open3_rois.insert("mode_diff_badge".to_string(), RoiRect { x: 212, y: 830, width: 316, height: 39 });
        result_open3_rois.insert("max_combo_badge".to_string(), RoiRect { x: 439, y: 591, width: 73, height: 73 });
        result_open3_rois.insert("score".to_string(), RoiRect { x: 211, y: 753, width: 317, height: 74 });
        scenes.insert(SceneType::ResultOpen3, SceneRoiConfig { rois: result_open3_rois });
 
        // ResultOpen2 ROI (오픈매치 2인)
        let mut result_open2_rois = HashMap::new();
        result_open2_rois.insert("jacket".to_string(), RoiRect { x: 705, y: 14, width: 60, height: 60 });
        result_open2_rois.insert("rate".to_string(), RoiRect { x: 403, y: 673, width: 107, height: 31 });
        result_open2_rois.insert("mode_diff_badge".to_string(), RoiRect { x: 312, y: 830, width: 315, height: 40 });
        result_open2_rois.insert("max_combo_badge".to_string(), RoiRect { x: 539, y: 591, width: 73, height: 73 });
        result_open2_rois.insert("score".to_string(), RoiRect { x: 311, y: 753, width: 320, height: 72 });
        scenes.insert(SceneType::ResultOpen2, SceneRoiConfig { rois: result_open2_rois });

        Self { scenes }
    }
}
