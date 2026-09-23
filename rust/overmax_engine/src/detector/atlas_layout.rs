use crate::capture::frame::CapturedFrame;
use crate::detector::roi_config::RawRoiRect;
use overmax_core::SceneType;

/// GPU 아틀라스 텍스처 규격 (512x512 RGBA8, 1 MB)
pub const ATLAS_WIDTH: u32 = 512;
pub const ATLAS_HEIGHT: u32 = 512;
pub const ATLAS_SLOT_COUNT: usize = 47;

/// 정적 아틀라스 내부의 개별 ROI 슬롯 배치 정보
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AtlasSlot {
    /// 해당 ROI가 속한 게임 씬
    pub scene: SceneType,
    /// ROI 식별자 이름
    pub name: &'static str,
    /// 1080p 16:9 게임 화면 기준 원본 좌표 (x, y, w, h)
    pub src_rect: RawRoiRect,
    /// 512x512 아틀라스 텍스처 내부의 정적 배치 좌표 (x, y, w, h)
    pub atlas_rect: RawRoiRect,
}

/// 컴파일 타임에 2D MaxRects 알고리즘으로 100% 무손실 배치된 43개 정적 슬롯 테이블 (Zero Heap Allocation)
pub const ATLAS_SLOTS: [AtlasSlot; 47] = [
    AtlasSlot {
        scene: SceneType::Freestyle,
        name: "gp_center_left",
        src_rect: RawRoiRect {
            x: 702,
            y: 80,
            width: 7,
            height: 257,
        },
        atlas_rect: RawRoiRect {
            x: 0,
            y: 0,
            width: 7,
            height: 257,
        },
    },
    AtlasSlot {
        scene: SceneType::Freestyle,
        name: "gp_center_right",
        src_rect: RawRoiRect {
            x: 1211,
            y: 80,
            width: 7,
            height: 257,
        },
        atlas_rect: RawRoiRect {
            x: 7,
            y: 0,
            width: 7,
            height: 257,
        },
    },
    AtlasSlot {
        scene: SceneType::Freestyle,
        name: "gp_left_left",
        src_rect: RawRoiRect {
            x: 102,
            y: 80,
            width: 7,
            height: 257,
        },
        atlas_rect: RawRoiRect {
            x: 14,
            y: 0,
            width: 7,
            height: 257,
        },
    },
    AtlasSlot {
        scene: SceneType::Freestyle,
        name: "gp_left_right",
        src_rect: RawRoiRect {
            x: 611,
            y: 80,
            width: 7,
            height: 257,
        },
        atlas_rect: RawRoiRect {
            x: 21,
            y: 0,
            width: 7,
            height: 257,
        },
    },
    AtlasSlot {
        scene: SceneType::Freestyle,
        name: "gp_right_left",
        src_rect: RawRoiRect {
            x: 1342,
            y: 80,
            width: 7,
            height: 257,
        },
        atlas_rect: RawRoiRect {
            x: 28,
            y: 0,
            width: 7,
            height: 257,
        },
    },
    AtlasSlot {
        scene: SceneType::Freestyle,
        name: "gp_right_right",
        src_rect: RawRoiRect {
            x: 1851,
            y: 80,
            width: 7,
            height: 257,
        },
        atlas_rect: RawRoiRect {
            x: 35,
            y: 0,
            width: 7,
            height: 257,
        },
    },
    AtlasSlot {
        scene: SceneType::ResultFreestyle,
        name: "mode_colorbar",
        src_rect: RawRoiRect {
            x: 52,
            y: 0,
            width: 22,
            height: 96,
        },
        atlas_rect: RawRoiRect {
            x: 0,
            y: 257,
            width: 22,
            height: 96,
        },
    },
    AtlasSlot {
        scene: SceneType::ResultFreestyle,
        name: "score",
        src_rect: RawRoiRect {
            x: 759,
            y: 710,
            width: 407,
            height: 94,
        },
        atlas_rect: RawRoiRect {
            x: 42,
            y: 0,
            width: 407,
            height: 94,
        },
    },
    AtlasSlot {
        scene: SceneType::ResultFreestyle,
        name: "max_combo_badge",
        src_rect: RawRoiRect {
            x: 1024,
            y: 521,
            width: 75,
            height: 75,
        },
        atlas_rect: RawRoiRect {
            x: 0,
            y: 353,
            width: 75,
            height: 75,
        },
    },
    AtlasSlot {
        scene: SceneType::ResultOpen3,
        name: "max_combo_badge",
        src_rect: RawRoiRect {
            x: 437,
            y: 591,
            width: 75,
            height: 75,
        },
        atlas_rect: RawRoiRect {
            x: 0,
            y: 428,
            width: 75,
            height: 75,
        },
    },
    AtlasSlot {
        scene: SceneType::ResultOpen2,
        name: "max_combo_badge",
        src_rect: RawRoiRect {
            x: 537,
            y: 591,
            width: 75,
            height: 75,
        },
        atlas_rect: RawRoiRect {
            x: 22,
            y: 257,
            width: 75,
            height: 75,
        },
    },
    AtlasSlot {
        scene: SceneType::ResultOpen3,
        name: "score",
        src_rect: RawRoiRect {
            x: 211,
            y: 753,
            width: 317,
            height: 74,
        },
        atlas_rect: RawRoiRect {
            x: 42,
            y: 94,
            width: 317,
            height: 74,
        },
    },
    AtlasSlot {
        scene: SceneType::ResultOpen2,
        name: "score",
        src_rect: RawRoiRect {
            x: 311,
            y: 753,
            width: 320,
            height: 72,
        },
        atlas_rect: RawRoiRect {
            x: 42,
            y: 168,
            width: 320,
            height: 72,
        },
    },
    AtlasSlot {
        scene: SceneType::ResultFreestyle,
        name: "mode_digit",
        src_rect: RawRoiRect {
            x: 78,
            y: 28,
            width: 50,
            height: 68,
        },
        atlas_rect: RawRoiRect {
            x: 359,
            y: 94,
            width: 50,
            height: 68,
        },
    },
    AtlasSlot {
        scene: SceneType::Freestyle,
        name: "jacket",
        src_rect: RawRoiRect {
            x: 710,
            y: 533,
            width: 64,
            height: 60,
        },
        atlas_rect: RawRoiRect {
            x: 409,
            y: 94,
            width: 64,
            height: 60,
        },
    },
    AtlasSlot {
        scene: SceneType::OpenMatch,
        name: "jacket",
        src_rect: RawRoiRect {
            x: 664,
            y: 533,
            width: 64,
            height: 60,
        },
        atlas_rect: RawRoiRect {
            x: 409,
            y: 154,
            width: 64,
            height: 60,
        },
    },
    AtlasSlot {
        scene: SceneType::ResultFreestyle,
        name: "jacket",
        src_rect: RawRoiRect {
            x: 705,
            y: 14,
            width: 60,
            height: 60,
        },
        atlas_rect: RawRoiRect {
            x: 449,
            y: 0,
            width: 60,
            height: 60,
        },
    },
    AtlasSlot {
        scene: SceneType::ResultOpen3,
        name: "player_panel",
        src_rect: RawRoiRect {
            x: 212,
            y: 830,
            width: 316,
            height: 40,
        },
        atlas_rect: RawRoiRect {
            x: 97,
            y: 240,
            width: 316,
            height: 40,
        },
    },
    AtlasSlot {
        scene: SceneType::ResultOpen2,
        name: "player_panel",
        src_rect: RawRoiRect {
            x: 312,
            y: 830,
            width: 316,
            height: 40,
        },
        atlas_rect: RawRoiRect {
            x: 97,
            y: 280,
            width: 316,
            height: 40,
        },
    },
    AtlasSlot {
        scene: SceneType::Freestyle,
        name: "max_combo_badge",
        src_rect: RawRoiRect {
            x: 409,
            y: 585,
            width: 36,
            height: 36,
        },
        atlas_rect: RawRoiRect {
            x: 473,
            y: 60,
            width: 36,
            height: 36,
        },
    },
    AtlasSlot {
        scene: SceneType::OpenMatch,
        name: "max_combo_badge",
        src_rect: RawRoiRect {
            x: 398,
            y: 601,
            width: 36,
            height: 36,
        },
        atlas_rect: RawRoiRect {
            x: 473,
            y: 96,
            width: 36,
            height: 36,
        },
    },
    AtlasSlot {
        scene: SceneType::ResultFreestyle,
        name: "rate",
        src_rect: RawRoiRect {
            x: 891,
            y: 608,
            width: 129,
            height: 32,
        },
        atlas_rect: RawRoiRect {
            x: 75,
            y: 332,
            width: 129,
            height: 32,
        },
    },
    AtlasSlot {
        scene: SceneType::OpenMatch,
        name: "diff_panel_NM",
        src_rect: RawRoiRect {
            x: 82,
            y: 467,
            width: 116,
            height: 31,
        },
        atlas_rect: RawRoiRect {
            x: 75,
            y: 364,
            width: 116,
            height: 31,
        },
    },
    AtlasSlot {
        scene: SceneType::OpenMatch,
        name: "diff_panel_HD",
        src_rect: RawRoiRect {
            x: 202,
            y: 467,
            width: 116,
            height: 31,
        },
        atlas_rect: RawRoiRect {
            x: 75,
            y: 395,
            width: 116,
            height: 31,
        },
    },
    AtlasSlot {
        scene: SceneType::OpenMatch,
        name: "diff_panel_MX",
        src_rect: RawRoiRect {
            x: 322,
            y: 467,
            width: 116,
            height: 31,
        },
        atlas_rect: RawRoiRect {
            x: 75,
            y: 426,
            width: 116,
            height: 31,
        },
    },
    AtlasSlot {
        scene: SceneType::OpenMatch,
        name: "diff_panel_SC",
        src_rect: RawRoiRect {
            x: 442,
            y: 467,
            width: 116,
            height: 31,
        },
        atlas_rect: RawRoiRect {
            x: 75,
            y: 457,
            width: 116,
            height: 31,
        },
    },
    AtlasSlot {
        scene: SceneType::ResultOpen2,
        name: "rate",
        src_rect: RawRoiRect {
            x: 403,
            y: 673,
            width: 107,
            height: 31,
        },
        atlas_rect: RawRoiRect {
            x: 191,
            y: 364,
            width: 107,
            height: 31,
        },
    },
    AtlasSlot {
        scene: SceneType::ResultOpen3,
        name: "rate",
        src_rect: RawRoiRect {
            x: 293,
            y: 673,
            width: 107,
            height: 30,
        },
        atlas_rect: RawRoiRect {
            x: 204,
            y: 320,
            width: 107,
            height: 30,
        },
    },
    AtlasSlot {
        scene: SceneType::Freestyle,
        name: "pause_title",
        src_rect: RawRoiRect {
            x: 731,
            y: 177,
            width: 148,
            height: 28,
        },
        atlas_rect: RawRoiRect {
            x: 311,
            y: 320,
            width: 148,
            height: 28,
        },
    },
    AtlasSlot {
        scene: SceneType::Freestyle,
        name: "diff_panel_NM",
        src_rect: RawRoiRect {
            x: 98,
            y: 488,
            width: 110,
            height: 28,
        },
        atlas_rect: RawRoiRect {
            x: 191,
            y: 395,
            width: 110,
            height: 28,
        },
    },
    AtlasSlot {
        scene: SceneType::Freestyle,
        name: "diff_panel_HD",
        src_rect: RawRoiRect {
            x: 218,
            y: 488,
            width: 110,
            height: 28,
        },
        atlas_rect: RawRoiRect {
            x: 298,
            y: 350,
            width: 110,
            height: 28,
        },
    },
    AtlasSlot {
        scene: SceneType::Freestyle,
        name: "diff_panel_MX",
        src_rect: RawRoiRect {
            x: 338,
            y: 488,
            width: 110,
            height: 28,
        },
        atlas_rect: RawRoiRect {
            x: 191,
            y: 423,
            width: 110,
            height: 28,
        },
    },
    AtlasSlot {
        scene: SceneType::Freestyle,
        name: "diff_panel_SC",
        src_rect: RawRoiRect {
            x: 458,
            y: 488,
            width: 110,
            height: 28,
        },
        atlas_rect: RawRoiRect {
            x: 191,
            y: 451,
            width: 110,
            height: 28,
        },
    },
    AtlasSlot {
        scene: SceneType::Freestyle,
        name: "score",
        src_rect: RawRoiRect {
            x: 173,
            y: 558,
            width: 104,
            height: 24,
        },
        atlas_rect: RawRoiRect {
            x: 408,
            y: 348,
            width: 104,
            height: 24,
        },
    },
    AtlasSlot {
        scene: SceneType::Freestyle,
        name: "rate",
        src_rect: RawRoiRect {
            x: 172,
            y: 583,
            width: 104,
            height: 22,
        },
        atlas_rect: RawRoiRect {
            x: 408,
            y: 372,
            width: 104,
            height: 22,
        },
    },
    AtlasSlot {
        scene: SceneType::OpenMatch,
        name: "score",
        src_rect: RawRoiRect {
            x: 77,
            y: 558,
            width: 106,
            height: 20,
        },
        atlas_rect: RawRoiRect {
            x: 301,
            y: 378,
            width: 106,
            height: 20,
        },
    },
    AtlasSlot {
        scene: SceneType::OpenMatch,
        name: "rate",
        src_rect: RawRoiRect {
            x: 197,
            y: 558,
            width: 103,
            height: 20,
        },
        atlas_rect: RawRoiRect {
            x: 407,
            y: 394,
            width: 103,
            height: 20,
        },
    },
    AtlasSlot {
        scene: SceneType::ResultOpen3,
        name: "openmatch_diff",
        src_rect: RawRoiRect {
            x: 410,
            y: 841,
            width: 106,
            height: 18,
        },
        atlas_rect: RawRoiRect {
            x: 301,
            y: 398,
            width: 106,
            height: 18,
        },
    },
    AtlasSlot {
        scene: SceneType::ResultOpen2,
        name: "openmatch_diff",
        src_rect: RawRoiRect {
            x: 510,
            y: 841,
            width: 106,
            height: 18,
        },
        atlas_rect: RawRoiRect {
            x: 75,
            y: 488,
            width: 106,
            height: 18,
        },
    },
    AtlasSlot {
        scene: SceneType::ResultFreestyle,
        name: "diff_panel_NM",
        src_rect: RawRoiRect {
            x: 709,
            y: 86,
            width: 90,
            height: 18,
        },
        atlas_rect: RawRoiRect {
            x: 181,
            y: 488,
            width: 90,
            height: 18,
        },
    },
    AtlasSlot {
        scene: SceneType::ResultFreestyle,
        name: "diff_panel_HD",
        src_rect: RawRoiRect {
            x: 829,
            y: 86,
            width: 90,
            height: 18,
        },
        atlas_rect: RawRoiRect {
            x: 362,
            y: 214,
            width: 90,
            height: 18,
        },
    },
    AtlasSlot {
        scene: SceneType::ResultFreestyle,
        name: "diff_panel_MX",
        src_rect: RawRoiRect {
            x: 949,
            y: 86,
            width: 90,
            height: 18,
        },
        atlas_rect: RawRoiRect {
            x: 413,
            y: 232,
            width: 90,
            height: 18,
        },
    },
    AtlasSlot {
        scene: SceneType::ResultFreestyle,
        name: "diff_panel_SC",
        src_rect: RawRoiRect {
            x: 1069,
            y: 86,
            width: 90,
            height: 18,
        },
        atlas_rect: RawRoiRect {
            x: 413,
            y: 250,
            width: 90,
            height: 18,
        },
    },
    AtlasSlot {
        scene: SceneType::Freestyle,
        name: "btn_mode",
        src_rect: RawRoiRect {
            x: 80,
            y: 130,
            width: 5,
            height: 5,
        },
        atlas_rect: RawRoiRect {
            x: 359,
            y: 162,
            width: 5,
            height: 5,
        },
    },
    AtlasSlot {
        scene: SceneType::OpenMatch,
        name: "btn_mode",
        src_rect: RawRoiRect {
            x: 60,
            y: 130,
            width: 5,
            height: 5,
        },
        atlas_rect: RawRoiRect {
            x: 0,
            y: 506,
            width: 5,
            height: 5,
        },
    },
    AtlasSlot {
        scene: SceneType::ResultOpen3,
        name: "openmatch_mode",
        src_rect: RawRoiRect {
            x: 212,
            y: 830,
            width: 5,
            height: 5,
        },
        atlas_rect: RawRoiRect {
            x: 5,
            y: 506,
            width: 5,
            height: 5,
        },
    },
    AtlasSlot {
        scene: SceneType::ResultOpen2,
        name: "openmatch_mode",
        src_rect: RawRoiRect {
            x: 312,
            y: 830,
            width: 5,
            height: 5,
        },
        atlas_rect: RawRoiRect {
            x: 10,
            y: 506,
            width: 5,
            height: 5,
        },
    },
];
/// 1080p 프레임(`CapturedFrame`)에서 43개 ROI 슬롯을 복사하여
/// 512x512 정적 아틀라스 프레임을 CPU 상에서 생성합니다.
///
/// 이는 CPU 오프라인 검증 및 테스트 하네스를 위한 무손실 가상 아틀라스 빌더입니다.
pub fn build_virtual_atlas(frame: &CapturedFrame) -> CapturedFrame {
    let mut atlas_bgra = vec![0u8; (ATLAS_WIDTH * ATLAS_HEIGHT * 4) as usize];
    let frame_w = frame.width;
    let frame_h = frame.height;
    let atlas_stride = (ATLAS_WIDTH * 4) as usize;
    let src_stride = (frame_w * 4) as usize;

    for slot in &ATLAS_SLOTS {
        let sx = slot.src_rect.x;
        let sy = slot.src_rect.y;
        let sw = slot.src_rect.width;
        let sh = slot.src_rect.height;

        let ax = slot.atlas_rect.x;
        let ay = slot.atlas_rect.y;

        if sx < 0 || sy < 0 || sx + sw > frame_w || sy + sh > frame_h {
            continue;
        }

        let copy_bytes = (sw * 4) as usize;
        for row in 0..sh {
            let src_y = (sy + row) as usize;
            let dst_y = (ay + row) as usize;

            let src_idx = (src_y * src_stride) + (sx as usize * 4);
            let dst_idx = (dst_y * atlas_stride) + (ax as usize * 4);

            atlas_bgra[dst_idx..dst_idx + copy_bytes]
                .copy_from_slice(&frame.bgra[src_idx..src_idx + copy_bytes]);
        }
    }

    CapturedFrame {
        width: ATLAS_WIDTH as i32,
        height: ATLAS_HEIGHT as i32,
        bgra: atlas_bgra,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detector::roi_config::GlobalRoiConfig;
    use overmax_core::Difficulty;

    #[test]
    fn test_all_slots_within_atlas_bounds() {
        for slot in &ATLAS_SLOTS {
            assert!(
                slot.atlas_rect.x >= 0,
                "Slot {} ({:?}) negative atlas x: {}",
                slot.name,
                slot.scene,
                slot.atlas_rect.x
            );
            assert!(
                slot.atlas_rect.y >= 0,
                "Slot {} ({:?}) negative atlas y: {}",
                slot.name,
                slot.scene,
                slot.atlas_rect.y
            );
            assert!(
                slot.atlas_rect.x + slot.atlas_rect.width <= ATLAS_WIDTH as i32,
                "Slot {} ({:?}) exceeds ATLAS_WIDTH: {} + {} = {} > {}",
                slot.name,
                slot.scene,
                slot.atlas_rect.x,
                slot.atlas_rect.width,
                slot.atlas_rect.x + slot.atlas_rect.width,
                ATLAS_WIDTH
            );
            assert!(
                slot.atlas_rect.y + slot.atlas_rect.height <= ATLAS_HEIGHT as i32,
                "Slot {} ({:?}) exceeds ATLAS_HEIGHT: {} + {} = {} > {}",
                slot.name,
                slot.scene,
                slot.atlas_rect.y,
                slot.atlas_rect.height,
                slot.atlas_rect.y + slot.atlas_rect.height,
                ATLAS_HEIGHT
            );
        }
    }

    #[test]
    fn test_no_overlapping_slots_in_atlas() {
        for (i, s1) in ATLAS_SLOTS.iter().enumerate() {
            let r1 = s1.atlas_rect;
            for s2 in ATLAS_SLOTS.iter().skip(i + 1) {
                let r2 = s2.atlas_rect;

                let overlap = !(r1.x + r1.width <= r2.x
                    || r2.x + r2.width <= r1.x
                    || r1.y + r1.height <= r2.y
                    || r2.y + r2.height <= r1.y);

                assert!(
                    !overlap,
                    "Overlap detected between slot {} ({:?}) at {:?} and slot {} ({:?}) at {:?}",
                    s1.name, s1.scene, r1, s2.name, s2.scene, r2
                );
            }
        }
    }

    #[test]
    fn test_positive_slot_dimensions() {
        for slot in &ATLAS_SLOTS {
            assert!(slot.src_rect.width > 0);
            assert!(slot.src_rect.height > 0);
            assert_eq!(slot.src_rect.width, slot.atlas_rect.width);
            assert_eq!(slot.src_rect.height, slot.atlas_rect.height);
        }
    }

    #[test]
    fn test_src_rect_matches_global_roi_config() {
        let global_config = GlobalRoiConfig::default();

        for slot in &ATLAS_SLOTS {
            let scene_config = global_config
                .scenes
                .get(&slot.scene)
                .unwrap_or_else(|| panic!("Scene {:?} not found in GlobalRoiConfig", slot.scene));

            if slot.name.starts_with("diff_panel_") {
                let diff_name = slot.name.strip_prefix("diff_panel_").unwrap();
                let diff = match diff_name {
                    "NM" => Difficulty::NM,
                    "HD" => Difficulty::HD,
                    "MX" => Difficulty::MX,
                    "SC" => Difficulty::SC,
                    _ => panic!("Unknown diff: {}", diff_name),
                };
                let base_diff_rect = scene_config
                    .rois
                    .get("diff_panel")
                    .unwrap_or_else(|| panic!("diff_panel not found in {:?}", slot.scene));
                let offset = match diff {
                    Difficulty::NM => 0,
                    Difficulty::HD => 120,
                    Difficulty::MX => 240,
                    Difficulty::SC => 360,
                };
                assert_eq!(
                    slot.src_rect.x,
                    base_diff_rect.x + offset,
                    "Slot {} diff x mismatch",
                    slot.name
                );
                assert_eq!(
                    slot.src_rect.y, base_diff_rect.y,
                    "Slot {} diff y mismatch",
                    slot.name
                );
                assert_eq!(
                    slot.src_rect.width, base_diff_rect.width,
                    "Slot {} diff width mismatch",
                    slot.name
                );
                assert_eq!(
                    slot.src_rect.height, base_diff_rect.height,
                    "Slot {} diff height mismatch",
                    slot.name
                );
            } else {
                let expected = scene_config.rois.get(slot.name).unwrap_or_else(|| {
                    panic!("ROI {} not found in scene {:?}", slot.name, slot.scene)
                });
                if (slot.scene == SceneType::Freestyle || slot.scene == SceneType::OpenMatch)
                    && slot.name == "jacket"
                {
                    // 카테고리 띠(4px) 확장에 따라 src_rect.width가 64(60 + 4)로 설정됨을 검증
                    assert_eq!(slot.src_rect.x, expected.x);
                    assert_eq!(slot.src_rect.y, expected.y);
                    assert_eq!(slot.src_rect.width, expected.width + 4);
                    assert_eq!(slot.src_rect.height, expected.height);
                } else if slot.scene == SceneType::ResultFreestyle && slot.name == "mode_colorbar" {
                    // 결과창 외곽선 마진(8px) 확장에 따라 src_rect.x가 52(60 - 8), width가 22(6 + 16)로 설정됨을 검증
                    assert_eq!(slot.src_rect.x, expected.x - 8);
                    assert_eq!(slot.src_rect.y, expected.y);
                    assert_eq!(slot.src_rect.width, expected.width + 16);
                    assert_eq!(slot.src_rect.height, expected.height);
                } else {
                    assert_eq!(
                        slot.src_rect, *expected,
                        "Src rect mismatch for {} in {:?}",
                        slot.name, slot.scene
                    );
                }
            }
        }
    }

    #[test]
    fn test_virtual_atlas_pixel_perfect_identity() {
        use crate::capture::frame_utils::crop_roi;
        use crate::detector::atlas_translator::AtlasTranslator;

        // 1. 패턴 데이터로 채워진 1920x1080 테스트 프레임 생성
        let w = 1920;
        let h = 1080;
        let mut bgra = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                bgra.push((x & 0xFF) as u8); // B
                bgra.push((y & 0xFF) as u8); // G
                bgra.push(((x ^ y) & 0xFF) as u8); // R
                bgra.push(255); // A
            }
        }
        let original_frame = CapturedFrame {
            width: w,
            height: h,
            bgra,
        };

        // 2. CPU 가상 아틀라스 생성
        let atlas_frame = build_virtual_atlas(&original_frame);
        assert_eq!(atlas_frame.width, ATLAS_WIDTH as i32);
        assert_eq!(atlas_frame.height, ATLAS_HEIGHT as i32);

        // 3. 43개 슬롯 전수에 대해 원본 크롭 vs 아틀라스 크롭 픽셀 바이트 일치 검증
        for slot in &ATLAS_SLOTS {
            let orig_crop = crop_roi(&original_frame, slot.src_rect.into())
                .unwrap_or_else(|| panic!("Failed to crop original for {}", slot.name));

            let atlas_slot_crop = crop_roi(&atlas_frame, slot.atlas_rect.into())
                .unwrap_or_else(|| panic!("Failed to crop atlas slot for {}", slot.name));

            assert_eq!(orig_crop.width, atlas_slot_crop.width);
            assert_eq!(orig_crop.height, atlas_slot_crop.height);

            // 모든 행, 모든 바이트가 100% 동일한지 전수 비교
            for y in 0..orig_crop.height {
                assert_eq!(
                    orig_crop.row(y),
                    atlas_slot_crop.row(y),
                    "Pixel mismatch at row {} for slot {} in {:?}",
                    y,
                    slot.name,
                    slot.scene
                );
            }

            // 그리고 AtlasTranslator가 반환하는 내부 ROI도 유효하고 슬롯 내부에 정상 크롭되는지 검증
            let atlas_roi = AtlasTranslator::get_roi_for_scene(slot.name, slot.scene)
                .unwrap_or_else(|| panic!("Failed to resolve atlas roi for {}", slot.name));
            let atlas_crop = crop_roi(&atlas_frame, atlas_roi)
                .unwrap_or_else(|| panic!("Failed to crop atlas for {}", slot.name));
            assert!(atlas_crop.width > 0 && atlas_crop.height > 0);
        }
    }

    #[test]
    fn test_virtual_atlas_diff_panel_pixel_perfect_identity() {
        use crate::capture::frame_utils::crop_roi;
        use crate::detector::atlas_translator::AtlasTranslator;
        use crate::detector::roi::RoiManager;

        let w = 1920;
        let h = 1080;
        let mut bgra = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                bgra.push(((x * 7) & 0xFF) as u8);
                bgra.push(((y * 13) & 0xFF) as u8);
                bgra.push(((x + y) & 0xFF) as u8);
                bgra.push(255);
            }
        }
        let original_frame = CapturedFrame {
            width: w,
            height: h,
            bgra,
        };

        let atlas_frame = build_virtual_atlas(&original_frame);
        let roi_manager = RoiManager::new(1920, 1080);

        for scene in [
            SceneType::Freestyle,
            SceneType::OpenMatch,
            SceneType::LadderMatch,
            SceneType::ResultFreestyle,
        ] {
            for diff in Difficulty::ALL {
                let orig_roi = roi_manager
                    .get_diff_panel_roi_for_scene(diff, scene)
                    .unwrap();
                let orig_crop = crop_roi(&original_frame, orig_roi).unwrap();

                let atlas_roi = AtlasTranslator::get_diff_panel_roi_for_scene(diff, scene).unwrap();
                let atlas_crop = crop_roi(&atlas_frame, atlas_roi).unwrap();

                assert_eq!(orig_crop.width, atlas_crop.width);
                assert_eq!(orig_crop.height, atlas_crop.height);

                for y in 0..orig_crop.height {
                    assert_eq!(
                        orig_crop.row(y),
                        atlas_crop.row(y),
                        "Diff panel pixel mismatch at row {} for diff {:?} in {:?}",
                        y,
                        diff,
                        scene
                    );
                }
            }
        }
    }
}
