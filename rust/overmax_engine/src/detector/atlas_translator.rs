use crate::capture::frame::CapturedFrame;
use crate::capture::frame_utils::{crop_roi, ImageView};
use crate::detector::roi::RoiRect;
use overmax_core::{Difficulty, SceneType};

/// 컴파일 타임에 100% 결정되는 제로 코스트 O(1) 아틀라스 트랜슬레이터 (Zero Runtime Overhead)
pub struct AtlasTranslator;

impl AtlasTranslator {
    /// 512x512 아틀라스 내부에서 지정된 씬과 이름에 해당하는 ROI 사각형을 반환합니다.
    ///
    /// 컴파일러가 완벽한 O(1) 점프 테이블로 인라인화하므로 런타임 룩업 비용이 0입니다.
    #[inline(always)]
    pub const fn get_roi_for_scene(name: &str, scene: SceneType) -> Option<RoiRect> {
        match (scene, name.as_bytes()) {
            // [ResultFreestyle]
            (SceneType::ResultFreestyle, b"mode_colorbar") => Some(RoiRect {
                x1: 8,
                y1: 257,
                x2: 14,
                y2: 353,
            }),
            (SceneType::ResultFreestyle, b"score") => Some(RoiRect {
                x1: 42,
                y1: 0,
                x2: 449,
                y2: 94,
            }),
            (SceneType::ResultFreestyle, b"max_combo_badge") => Some(RoiRect {
                x1: 0,
                y1: 353,
                x2: 75,
                y2: 428,
            }),
            (SceneType::ResultFreestyle, b"mode_digit") => Some(RoiRect {
                x1: 359,
                y1: 94,
                x2: 409,
                y2: 162,
            }),
            (
                SceneType::ResultFreestyle | SceneType::ResultOpen3 | SceneType::ResultOpen2,
                b"jacket",
            ) => Some(RoiRect {
                x1: 449,
                y1: 0,
                x2: 509,
                y2: 60,
            }),
            (SceneType::ResultFreestyle, b"rate") => Some(RoiRect {
                x1: 75,
                y1: 332,
                x2: 204,
                y2: 364,
            }),
            (SceneType::ResultFreestyle, b"diff_panel_NM") => Some(RoiRect {
                x1: 313,
                y1: 271,
                x2: 403,
                y2: 289,
            }),
            (SceneType::ResultFreestyle, b"diff_panel_HD") => Some(RoiRect {
                x1: 362,
                y1: 214,
                x2: 452,
                y2: 232,
            }),
            (SceneType::ResultFreestyle, b"diff_panel_MX") => Some(RoiRect {
                x1: 413,
                y1: 232,
                x2: 503,
                y2: 250,
            }),
            (SceneType::ResultFreestyle, b"diff_panel_SC") => Some(RoiRect {
                x1: 413,
                y1: 250,
                x2: 503,
                y2: 268,
            }),
            (SceneType::ResultFreestyle, b"diff_panel") => Some(RoiRect {
                x1: 313,
                y1: 271,
                x2: 403,
                y2: 289,
            }),
            // [ResultOpen3]
            (SceneType::ResultOpen3, b"max_combo_badge") => Some(RoiRect {
                x1: 0,
                y1: 428,
                x2: 75,
                y2: 503,
            }),
            (SceneType::ResultOpen3, b"score") => Some(RoiRect {
                x1: 42,
                y1: 94,
                x2: 359,
                y2: 168,
            }),
            (SceneType::ResultOpen3, b"player_panel") => Some(RoiRect {
                x1: 88,
                y1: 464,
                x2: 404,
                y2: 504,
            }),
            (SceneType::ResultOpen3, b"rate") => Some(RoiRect {
                x1: 204,
                y1: 320,
                x2: 311,
                y2: 350,
            }),
            (SceneType::ResultOpen3, b"openmatch_diff") => Some(RoiRect {
                x1: 301,
                y1: 398,
                x2: 407,
                y2: 416,
            }),
            (SceneType::ResultOpen3, b"openmatch_mode") => Some(RoiRect {
                x1: 5,
                y1: 506,
                x2: 10,
                y2: 511,
            }),
            // [ResultOpen2]
            (SceneType::ResultOpen2, b"max_combo_badge") => Some(RoiRect {
                x1: 22,
                y1: 257,
                x2: 97,
                y2: 332,
            }),
            (SceneType::ResultOpen2, b"score") => Some(RoiRect {
                x1: 42,
                y1: 168,
                x2: 362,
                y2: 240,
            }),
            (SceneType::ResultOpen2, b"player_panel") => Some(RoiRect {
                x1: 188,
                y1: 464,
                x2: 504,
                y2: 504,
            }),
            (SceneType::ResultOpen2, b"rate") => Some(RoiRect {
                x1: 191,
                y1: 364,
                x2: 298,
                y2: 395,
            }),
            (SceneType::ResultOpen2, b"openmatch_diff") => Some(RoiRect {
                x1: 207,
                y1: 271,
                x2: 313,
                y2: 289,
            }),
            (SceneType::ResultOpen2, b"openmatch_mode") => Some(RoiRect {
                x1: 10,
                y1: 506,
                x2: 15,
                y2: 511,
            }),
            // [Freestyle]
            (SceneType::Unknown, b"gp_center_left") => Some(RoiRect {
                x1: 0,
                y1: 0,
                x2: 7,
                y2: 257,
            }),
            (SceneType::Unknown, b"gp_center_right") => Some(RoiRect {
                x1: 7,
                y1: 0,
                x2: 14,
                y2: 257,
            }),
            (SceneType::Unknown, b"gp_left_left") => Some(RoiRect {
                x1: 14,
                y1: 0,
                x2: 21,
                y2: 257,
            }),
            (SceneType::Unknown, b"gp_left_right") => Some(RoiRect {
                x1: 21,
                y1: 0,
                x2: 28,
                y2: 257,
            }),
            (SceneType::Unknown, b"gp_right_left") => Some(RoiRect {
                x1: 28,
                y1: 0,
                x2: 35,
                y2: 257,
            }),
            (SceneType::Unknown, b"gp_right_right") => Some(RoiRect {
                x1: 35,
                y1: 0,
                x2: 42,
                y2: 257,
            }),
            (SceneType::Freestyle, b"jacket") => Some(RoiRect {
                x1: 409,
                y1: 94,
                x2: 469,
                y2: 154,
            }),
            (SceneType::Freestyle, b"max_combo_badge") => Some(RoiRect {
                x1: 473,
                y1: 60,
                x2: 509,
                y2: 96,
            }),
            (SceneType::Unknown, b"pause_title") => Some(RoiRect {
                x1: 311,
                y1: 320,
                x2: 459,
                y2: 348,
            }),
            (SceneType::Freestyle, b"diff_panel_NM") => Some(RoiRect {
                x1: 191,
                y1: 395,
                x2: 301,
                y2: 423,
            }),
            (SceneType::Freestyle, b"diff_panel_HD") => Some(RoiRect {
                x1: 298,
                y1: 350,
                x2: 408,
                y2: 378,
            }),
            (SceneType::Freestyle, b"diff_panel_MX") => Some(RoiRect {
                x1: 191,
                y1: 423,
                x2: 301,
                y2: 451,
            }),
            (SceneType::Freestyle, b"diff_panel_SC") => Some(RoiRect {
                x1: 97,
                y1: 271,
                x2: 207,
                y2: 299,
            }),
            (SceneType::Freestyle, b"diff_panel") => Some(RoiRect {
                x1: 191,
                y1: 395,
                x2: 301,
                y2: 423,
            }),
            (SceneType::Freestyle, b"score") => Some(RoiRect {
                x1: 408,
                y1: 348,
                x2: 512,
                y2: 372,
            }),
            (SceneType::Freestyle, b"rate") => Some(RoiRect {
                x1: 408,
                y1: 372,
                x2: 512,
                y2: 394,
            }),
            (SceneType::Freestyle, b"btn_mode") => Some(RoiRect {
                x1: 359,
                y1: 162,
                x2: 364,
                y2: 167,
            }),
            // [OpenMatch]
            (SceneType::OpenMatch | SceneType::LadderMatch, b"jacket") => Some(RoiRect {
                x1: 409,
                y1: 154,
                x2: 469,
                y2: 214,
            }),
            (SceneType::OpenMatch | SceneType::LadderMatch, b"max_combo_badge") => Some(RoiRect {
                x1: 473,
                y1: 96,
                x2: 509,
                y2: 132,
            }),
            (SceneType::OpenMatch | SceneType::LadderMatch, b"diff_panel_NM") => Some(RoiRect {
                x1: 75,
                y1: 364,
                x2: 191,
                y2: 395,
            }),
            (SceneType::OpenMatch | SceneType::LadderMatch, b"diff_panel_HD") => Some(RoiRect {
                x1: 75,
                y1: 395,
                x2: 191,
                y2: 426,
            }),
            (SceneType::OpenMatch | SceneType::LadderMatch, b"diff_panel_MX") => Some(RoiRect {
                x1: 97,
                y1: 240,
                x2: 213,
                y2: 271,
            }),
            (SceneType::OpenMatch | SceneType::LadderMatch, b"diff_panel_SC") => Some(RoiRect {
                x1: 213,
                y1: 240,
                x2: 329,
                y2: 271,
            }),
            (SceneType::OpenMatch | SceneType::LadderMatch, b"diff_panel") => Some(RoiRect {
                x1: 75,
                y1: 364,
                x2: 191,
                y2: 395,
            }),
            (SceneType::OpenMatch | SceneType::LadderMatch, b"score") => Some(RoiRect {
                x1: 301,
                y1: 378,
                x2: 407,
                y2: 398,
            }),
            (SceneType::OpenMatch | SceneType::LadderMatch, b"rate") => Some(RoiRect {
                x1: 407,
                y1: 394,
                x2: 510,
                y2: 414,
            }),
            (SceneType::OpenMatch | SceneType::LadderMatch, b"btn_mode") => Some(RoiRect {
                x1: 0,
                y1: 506,
                x2: 5,
                y2: 511,
            }),
            _ => None,
        }
    }

    /// 지정된 씬과 난이도에 해당하는 아틀라스 내부의 diff_panel ROI를 반환합니다.
    #[inline(always)]
    pub const fn get_diff_panel_roi_for_scene(
        diff: Difficulty,
        scene: SceneType,
    ) -> Option<RoiRect> {
        match (scene, diff) {
            (SceneType::OpenMatch | SceneType::LadderMatch, Difficulty::NM) => Some(RoiRect {
                x1: 75,
                y1: 364,
                x2: 191,
                y2: 395,
            }),
            (SceneType::OpenMatch | SceneType::LadderMatch, Difficulty::HD) => Some(RoiRect {
                x1: 75,
                y1: 395,
                x2: 191,
                y2: 426,
            }),
            (SceneType::OpenMatch | SceneType::LadderMatch, Difficulty::MX) => Some(RoiRect {
                x1: 97,
                y1: 240,
                x2: 213,
                y2: 271,
            }),
            (SceneType::OpenMatch | SceneType::LadderMatch, Difficulty::SC) => Some(RoiRect {
                x1: 213,
                y1: 240,
                x2: 329,
                y2: 271,
            }),
            (SceneType::Freestyle, Difficulty::NM) => Some(RoiRect {
                x1: 191,
                y1: 395,
                x2: 301,
                y2: 423,
            }),
            (SceneType::Freestyle, Difficulty::HD) => Some(RoiRect {
                x1: 298,
                y1: 350,
                x2: 408,
                y2: 378,
            }),
            (SceneType::Freestyle, Difficulty::MX) => Some(RoiRect {
                x1: 191,
                y1: 423,
                x2: 301,
                y2: 451,
            }),
            (SceneType::Freestyle, Difficulty::SC) => Some(RoiRect {
                x1: 97,
                y1: 271,
                x2: 207,
                y2: 299,
            }),
            (SceneType::ResultFreestyle, Difficulty::NM) => Some(RoiRect {
                x1: 313,
                y1: 271,
                x2: 403,
                y2: 289,
            }),
            (SceneType::ResultFreestyle, Difficulty::HD) => Some(RoiRect {
                x1: 362,
                y1: 214,
                x2: 452,
                y2: 232,
            }),
            (SceneType::ResultFreestyle, Difficulty::MX) => Some(RoiRect {
                x1: 413,
                y1: 232,
                x2: 503,
                y2: 250,
            }),
            (SceneType::ResultFreestyle, Difficulty::SC) => Some(RoiRect {
                x1: 413,
                y1: 250,
                x2: 503,
                y2: 268,
            }),
            _ => None,
        }
    }

    /// 아틀라스 프레임(512x512)에서 특정 씬의 ROI를 직접 크롭하여 ImageView로 반환합니다.
    #[inline]
    pub fn crop_roi<'a>(
        atlas_frame: &'a CapturedFrame,
        name: &str,
        scene: SceneType,
    ) -> Option<ImageView<'a>> {
        let roi = Self::get_roi_for_scene(name, scene)?;
        crop_roi(atlas_frame, roi)
    }

    /// 아틀라스 프레임에서 diff_panel ROI를 직접 크롭하여 ImageView로 반환합니다.
    #[inline]
    pub fn crop_diff_panel_roi<'a>(
        atlas_frame: &'a CapturedFrame,
        diff: Difficulty,
        scene: SceneType,
    ) -> Option<ImageView<'a>> {
        let roi = Self::get_diff_panel_roi_for_scene(diff, scene)?;
        crop_roi(atlas_frame, roi)
    }

    /// 기존 `RoiManager::and_then_roi`와 동일한 클로저 기반 호출 편의 인터페이스
    #[inline]
    pub fn and_then_roi<'a, T>(
        atlas_frame: &'a CapturedFrame,
        name: &str,
        scene: SceneType,
        f: impl FnOnce(&ImageView<'a>) -> Option<T>,
    ) -> Option<T> {
        Self::crop_roi(atlas_frame, name, scene)
            .as_ref()
            .and_then(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detector::atlas_layout::{ATLAS_HEIGHT, ATLAS_SLOTS, ATLAS_WIDTH};
    use crate::detector::roi::RoiManager;

    #[test]
    fn test_all_slots_resolvable_via_translator() {
        for slot in &ATLAS_SLOTS {
            let translated = AtlasTranslator::get_roi_for_scene(slot.name, slot.scene)
                .unwrap_or_else(|| {
                    panic!("Failed to resolve slot {} in {:?}", slot.name, slot.scene)
                });

            assert!(
                translated.x1 >= slot.atlas_rect.x,
                "x1 out of bounds for {} in {:?}",
                slot.name,
                slot.scene
            );
            assert!(
                translated.y1 >= slot.atlas_rect.y,
                "y1 out of bounds for {} in {:?}",
                slot.name,
                slot.scene
            );
            assert!(
                translated.x2 <= slot.atlas_rect.x + slot.atlas_rect.width,
                "x2 out of bounds for {} in {:?}",
                slot.name,
                slot.scene
            );
            assert!(
                translated.y2 <= slot.atlas_rect.y + slot.atlas_rect.height,
                "y2 out of bounds for {} in {:?}",
                slot.name,
                slot.scene
            );
        }
    }

    #[test]
    fn test_dimensions_match_roi_manager_1080p() {
        let roi_manager = RoiManager::new(1920, 1080);

        for slot in &ATLAS_SLOTS {
            let translated = AtlasTranslator::get_roi_for_scene(slot.name, slot.scene).unwrap();

            let original_roi = if slot.name.starts_with("diff_panel_") {
                let diff_name = slot.name.strip_prefix("diff_panel_").unwrap();
                let diff = match diff_name {
                    "NM" => Difficulty::NM,
                    "HD" => Difficulty::HD,
                    "MX" => Difficulty::MX,
                    "SC" => Difficulty::SC,
                    _ => panic!("Unknown diff: {}", diff_name),
                };
                roi_manager
                    .get_diff_panel_roi_for_scene(diff, slot.scene)
                    .unwrap()
            } else {
                roi_manager
                    .get_roi_for_scene(slot.name, slot.scene)
                    .unwrap()
            };

            assert_eq!(
                translated.width(),
                original_roi.width(),
                "Width mismatch for {} in {:?}: atlas={}, original={}",
                slot.name,
                slot.scene,
                translated.width(),
                original_roi.width()
            );
            assert_eq!(
                translated.height(),
                original_roi.height(),
                "Height mismatch for {} in {:?}: atlas={}, original={}",
                slot.name,
                slot.scene,
                translated.height(),
                original_roi.height()
            );
        }
    }

    #[test]
    fn test_diff_panel_translator_consistency() {
        for scene in [
            SceneType::Freestyle,
            SceneType::OpenMatch,
            SceneType::LadderMatch,
            SceneType::ResultFreestyle,
        ] {
            for diff in Difficulty::ALL {
                let direct = AtlasTranslator::get_diff_panel_roi_for_scene(diff, scene)
                    .unwrap_or_else(|| panic!("Diff {:?} missing for {:?}", diff, scene));

                let name = match diff {
                    Difficulty::NM => "diff_panel_NM",
                    Difficulty::HD => "diff_panel_HD",
                    Difficulty::MX => "diff_panel_MX",
                    Difficulty::SC => "diff_panel_SC",
                };
                let by_name = AtlasTranslator::get_roi_for_scene(name, scene)
                    .unwrap_or_else(|| panic!("Named diff {} missing for {:?}", name, scene));

                assert_eq!(direct, by_name);
                assert!(direct.x1 >= 0 && direct.x2 <= ATLAS_WIDTH as i32);
                assert!(direct.y1 >= 0 && direct.y2 <= ATLAS_HEIGHT as i32);
            }
        }
    }

    #[test]
    fn test_ladder_match_shares_open_match_atlas_coordinates() {
        for name in [
            "jacket",
            "rate",
            "score",
            "btn_mode",
            "max_combo_badge",
            "diff_panel_NM",
            "diff_panel_HD",
            "diff_panel_MX",
            "diff_panel_SC",
        ] {
            let open_match_roi =
                AtlasTranslator::get_roi_for_scene(name, SceneType::OpenMatch).unwrap();
            let ladder_match_roi =
                AtlasTranslator::get_roi_for_scene(name, SceneType::LadderMatch).unwrap();

            assert_eq!(
                open_match_roi, ladder_match_roi,
                "LadderMatch ROI mismatch with OpenMatch for {}",
                name
            );
        }
    }

    /// 아틀라스 슬롯이 없는 씬에 ROI 매핑이 생기면 안 된다.
    ///
    /// `get_roi_for_scene`이 `Some`을 반환하는데 `ATLAS_SLOTS`에 대응 슬롯이 없으면,
    /// 그 좌표는 어떤 픽셀도 담고 있지 않아 조용히 잘못된 데이터를 돌려준다.
    /// 슬롯 테이블을 순회하는 기존 테스트는 이 불일치를 잡지 못하므로,
    /// 여기서는 매핑 측에서 출발해 모든 씬을 검사한다.
    #[test]
    fn test_no_translator_mapping_without_backing_slot() {
        // LadderMatch는 OpenMatch와 좌표를 공유하고, diff_panel은 난이도 무지 별칭이다.
        // ResultOpen3/Open2의 jacket은 3씬의 src_rect가 동일해 ResultFreestyle 슬롯을 공유한다.
        // ResultOpen2의 player_panel은 Open3/Open2 합집합으로 확장된 ResultOpen3 슬롯을 공유한다.
        let shared: &[(SceneType, SceneType, &str)] = &[
            (SceneType::LadderMatch, SceneType::OpenMatch, ""),
            (SceneType::ResultOpen3, SceneType::ResultFreestyle, "jacket"),
            (SceneType::ResultOpen2, SceneType::ResultFreestyle, "jacket"),
            (
                SceneType::ResultOpen2,
                SceneType::ResultOpen3,
                "player_panel",
            ),
        ];
        let has_slot = |scene: SceneType, name: &str| {
            ATLAS_SLOTS
                .iter()
                .any(|slot| slot.scene == scene && slot.name == name)
        };
        // 공유 씬은 owner 씬의 슬롯으로 해석하고, 난이도 무지 별칭은 같은 씬의
        // diff_panel_NM 슬롯이 backing이 된다. 두 규칙은 겹칠 수 있다
        // (예: LadderMatch/diff_panel은 OpenMatch 슬롯도 없고 별칭 resolution만 유효).
        let is_backed = |scene: SceneType, name: &str| {
            let owner = shared
                .iter()
                .find(|(s, _, only)| *s == scene && (only.is_empty() || *only == name))
                .map(|(_, owner, _)| *owner)
                .unwrap_or(scene);
            has_slot(owner, name) || (name == "diff_panel" && has_slot(owner, "diff_panel_NM"))
        };

        let mut names: Vec<&str> = ATLAS_SLOTS.iter().map(|slot| slot.name).collect();
        names.push("diff_panel");
        names.sort_unstable();
        names.dedup();

        for scene in [
            SceneType::Unknown,
            SceneType::Freestyle,
            SceneType::Online,
            SceneType::OpenMatch,
            SceneType::LadderMatch,
            SceneType::ResultFreestyle,
            SceneType::ResultOpen3,
            SceneType::ResultOpen2,
            SceneType::Gameplay,
            SceneType::Paused,
        ] {
            for name in &names {
                if AtlasTranslator::get_roi_for_scene(name, scene).is_none() {
                    continue;
                }
                assert!(
                    is_backed(scene, name),
                    "{:?}/{} resolves via translator but has no backing atlas slot. \
                     A mapping without a slot returns coordinates holding unrelated pixels.",
                    scene,
                    name
                );
            }
        }
    }
}
