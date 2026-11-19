//! `frame::atlas_layout` 슬롯 표와 detector의 ROI 설정·아틀라스 해석이 맞물리는지 확인하는 테스트.
//! (슬롯 표는 frame에 있지만, 이 검사들은 detector 타입이 필요해 detector 쪽에 둔다.)

use crate::roi_config::GlobalRoiConfig;
use overmax_core::{Difficulty, SceneType};
use overmax_frame::CapturedFrame;
use overmax_frame::*;

#[test]
fn test_src_rect_matches_global_roi_config() {
    let global_config = GlobalRoiConfig::default();

    for slot in &ATLAS_SLOTS {
        if slot.scene == SceneType::Unknown {
            let expected = global_config
                .rois
                .get(slot.name)
                .unwrap_or_else(|| panic!("Global ROI {} not found in GlobalRoiConfig", slot.name));
            assert_eq!(
                slot.src_rect, *expected,
                "Global src rect mismatch for {}",
                slot.name
            );
            continue;
        }
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
            let expected = scene_config
                .rois
                .get(slot.name)
                .unwrap_or_else(|| panic!("ROI {} not found in scene {:?}", slot.name, slot.scene));
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
            } else if slot.scene == SceneType::ResultOpen3 && slot.name == "player_panel" {
                // Open3/Open2 패널 합집합 + 엣지 마진(8px) 확장 슬롯임을 검증
                let open2 = global_config
                    .scenes
                    .get(&SceneType::ResultOpen2)
                    .and_then(|c| c.rois.get("player_panel"))
                    .expect("player_panel not found in ResultOpen2");
                assert_eq!(slot.src_rect.x, expected.x - 8);
                assert_eq!(slot.src_rect.y, expected.y - 8);
                assert_eq!(
                    slot.src_rect.x + slot.src_rect.width,
                    open2.x + open2.width + 8
                );
                assert_eq!(slot.src_rect.height, expected.height + 16);
                assert_eq!(open2.y, expected.y);
                assert_eq!(open2.height, expected.height);
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
    use crate::atlas_translator::AtlasTranslator;
    use overmax_frame::crop_roi;

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
    use crate::atlas_translator::AtlasTranslator;
    use crate::roi::RoiManager;
    use overmax_frame::crop_roi;

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
