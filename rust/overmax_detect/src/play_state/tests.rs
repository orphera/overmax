use super::{
    detect_button_mode, paint_song_select_rate_zero, PlayStateDetector, RateInputChecksums,
};
use crate::roi::RoiManager;
use overmax_core::{PatternRecord, SceneType};
use overmax_cv::Bgr;
use overmax_frame::CapturedFrame;

#[test]
fn detects_button_mode_from_reference_color() {
    let mut frame = blank_frame();
    paint_rect(&mut frame, 80, 130, 85, 135, Bgr::from_rgb_hex(0x2D4F55)); // #2D4F55
    let mut rois = RoiManager::new(1920, 1080);
    rois.set_scene(SceneType::Freestyle);
    assert_eq!(detect_button_mode(&frame, &rois), Some(super::Mode::B4));
}

#[test]
fn marks_state_stable_after_repeated_valid_frames() {
    let mut detector = PlayStateDetector::new(3);
    let mut frame = blank_frame();
    paint_rect(&mut frame, 80, 130, 85, 135, Bgr::from_rgb_hex(0x2D4F55)); // #2D4F55
    paint_rect(&mut frame, 98, 488, 208, 516, Bgr::from_rgb_hex(0xDCDCDC)); // #DCDCDC
    let mut rois = RoiManager::new(1920, 1080);
    rois.set_scene(SceneType::Freestyle);
    paint_song_select_rate_zero(&mut frame); // 기록 없음 = 0.00%

    assert!(!detector.detect(&frame, &rois, Some(7), 1.0).0.is_stable);
    assert!(!detector.detect(&frame, &rois, Some(7), 2.0).0.is_stable);
    assert!(detector.detect(&frame, &rois, Some(7), 3.0).0.is_stable);
}

#[test]
fn result_mode_diff_remains_none_without_match() {
    let mut detector = PlayStateDetector::new(3);
    detector.result_mode_diff.clear();

    let frame = blank_frame();
    let mut rois = RoiManager::new(1920, 1080);
    rois.set_scene(SceneType::ResultFreestyle);

    // 결과창에서 mode_digit/diff_panel ROI가 없어 인식에 실패하면 None이어야 함
    let (state, event) = detector.detect(&frame, &rois, Some(7), 1.0);
    assert!(state.context.is_none());
    assert!(event.is_none());
}

#[test]
fn mode_diff_checksum_cache_only_refreshes_on_input_change() {
    let mut detector = PlayStateDetector::new(3);
    let mut frame = blank_frame();
    paint_rect(&mut frame, 80, 130, 85, 135, Bgr::from_rgb_hex(0x2D4F55)); // 4B
    paint_rect(&mut frame, 98, 488, 208, 516, Bgr::from_rgb_hex(0xDCDCDC)); // NORMAL diff
    let mut rois = RoiManager::new(1920, 1080);
    rois.set_scene(SceneType::Freestyle);

    let (state1, _) = detector.detect(&frame, &rois, Some(1), 1.0);
    assert_eq!(detector.mode_diff_cache.last_detect_ts, 1.0);
    assert!(detector.mode_diff_cache.last_checksum.is_some());

    // 동일 프레임에서 시간만 지나도 체크섬이 안 바뀌면 timestamp가 1.0으로 유지됨 (재계산 스킵)
    let (state2, _) = detector.detect(&frame, &rois, Some(1), 2.0);
    assert_eq!(detector.mode_diff_cache.last_detect_ts, 1.0);
    assert_eq!(
        state1.context.as_ref().map(|c| c.mode),
        state2.context.as_ref().map(|c| c.mode)
    );

    // 난이도 카드 하이라이트 변경 -> checksum 변경으로 3.0s 시점에 갱신됨
    paint_rect(&mut frame, 218, 488, 328, 516, Bgr::from_rgb_hex(0xFFFFFF));
    let (_state3, _) = detector.detect(&frame, &rois, Some(1), 3.0);
    assert_eq!(detector.mode_diff_cache.last_detect_ts, 3.0);
}

#[test]
fn song_select_emits_event_once_per_pattern() {
    let mut detector = PlayStateDetector::new(3);
    let mut frame = blank_frame();
    paint_rect(&mut frame, 80, 130, 85, 135, Bgr::from_rgb_hex(0x2D4F55)); // 4B
    paint_rect(&mut frame, 98, 488, 208, 516, Bgr::from_rgb_hex(0xDCDCDC)); // NORMAL diff
    let mut rois = RoiManager::new(1920, 1080);
    rois.set_scene(SceneType::Freestyle);
    paint_song_select_rate_zero(&mut frame); // 기록 없음 = 0.00%

    let emitted = (0..10)
        .filter(|i| {
            detector
                .detect(&frame, &rois, Some(7), f64::from(*i))
                .1
                .is_some()
        })
        .count();
    assert_eq!(emitted, 1, "선곡 화면에서 동일 패턴 이벤트가 중복 방출됨");
}

#[test]
fn result_exit_resets_event_latch_once() {
    let mut detector = PlayStateDetector::new(1);
    detector.result_mode_diff.mode.update(Some(super::Mode::B4));
    detector
        .result_mode_diff
        .diff
        .update(Some(super::Difficulty::NM));
    detector.rate_cache.set(
        Some(PatternRecord::Played {
            rate: 99.5,
            is_max_combo: false,
        }),
        None,
        0.0,
    );
    let frame = blank_frame();
    let mut rois = RoiManager::new(1920, 1080);
    rois.set_scene(SceneType::ResultFreestyle);
    assert!(detector.detect(&frame, &rois, Some(10), 1.0).1.is_some());
    assert!(detector.last_emitted_event.is_some());

    // 결과창 체류 중에는 래치 유지, 선곡 복귀 첫 프레임에서 비워짐
    rois.set_scene(SceneType::Freestyle);
    let _ = detector.detect(&frame, &rois, Some(10), 2.0);
    assert!(detector.last_emitted_event.is_none());
    assert!(detector.detected_cache_is_empty_for_test());
}

fn blank_frame() -> CapturedFrame {
    CapturedFrame {
        width: 1920,
        height: 1080,
        bgra: vec![0; 1920 * 1080 * 4],
    }
}

fn paint_rect(frame: &mut CapturedFrame, x1: i32, y1: i32, x2: i32, y2: i32, bgr: Bgr) {
    for y in y1..y2 {
        for x in x1..x2 {
            let idx = ((y * frame.width + x) * 4) as usize;
            bgr.write_to_bgra(&mut frame.bgra[idx..idx + 4], 255);
        }
    }
}

#[test]
fn rate_checksum_cache_only_refreshes_on_meaningful_input_change() {
    let previous = RateInputChecksums {
        score: 2_000,
        badge: 500,
    };

    assert_eq!(
        previous,
        RateInputChecksums {
            score: 2_049,
            badge: 549,
        }
    );
    assert_ne!(
        previous,
        RateInputChecksums {
            score: 2_051,
            badge: 500,
        }
    );
    assert_ne!(
        previous,
        RateInputChecksums {
            score: 2_000,
            badge: 551,
        }
    );
}

#[test]
fn test_verified_play_event_emits_once_per_result_session() {
    let mut detector = PlayStateDetector::new(3);
    detector.result_mode_diff.mode.update(Some(super::Mode::B4));
    detector
        .result_mode_diff
        .diff
        .update(Some(super::Difficulty::NM));
    detector.rate_cache.set(
        Some(PatternRecord::Played {
            rate: 99.5,
            is_max_combo: false,
        }),
        None,
        0.0,
    );

    let frame = blank_frame();
    let mut rois = RoiManager::new(1920, 1080);
    rois.set_scene(SceneType::ResultFreestyle);

    // Frame 1: Detecting (History 1/3) -> Event None
    let (state1, event1) = detector.detect(&frame, &rois, Some(10), 1.0);
    assert!(!state1.is_stable);
    assert!(event1.is_none());

    // Frame 2: Detecting (History 2/3) -> Event None
    let (state2, event2) = detector.detect(&frame, &rois, Some(10), 1.1);
    assert!(!state2.is_stable);
    assert!(event2.is_none());

    // Frame 3: Stable 확정 (History 3/3) -> Event 단 1회 방출!
    let (state3, event3) = detector.detect(&frame, &rois, Some(10), 1.2);
    assert!(state3.is_stable);
    assert!(event3.is_some());
    let event = event3.unwrap();
    assert_eq!(event.song_id, 10);
    assert_eq!(event.mode, super::Mode::B4);
    assert_eq!(event.diff, super::Difficulty::NM);
    assert_eq!(event.rate, 99.5);
    assert!(event.is_result_screen);

    // Frame 4: 결과창 체류 지속 -> Event 중복 방출 0회 (None)
    let (state4, event4) = detector.detect(&frame, &rois, Some(10), 1.3);
    assert!(state4.is_stable);
    assert!(
        event4.is_none(),
        "결과창 체류 중 중복 이벤트가 방출되면 안 됨"
    );

    // Frame 5: 결과창 체류 지속 -> Event 중복 방출 0회 (None)
    let (state5, event5) = detector.detect(&frame, &rois, Some(10), 1.4);
    assert!(state5.is_stable);
    assert!(event5.is_none());

    // 선곡 화면으로 복귀 -> 래치 자동 리셋
    rois.set_scene(SceneType::Freestyle);
    detector.clear_detected_cache();
    let (state_select, event_select) = detector.detect(&frame, &rois, Some(10), 2.0);
    assert!(event_select.is_none());
    assert!(!state_select.scene.is_result());

    // 다음 곡 플레이 후 새 결과창 진입
    rois.set_scene(SceneType::ResultFreestyle);
    detector.result_mode_diff.mode.update(Some(super::Mode::B6));
    detector
        .result_mode_diff
        .diff
        .update(Some(super::Difficulty::HD));
    detector.rate_cache.set(
        Some(PatternRecord::Played {
            rate: 98.0,
            is_max_combo: false,
        }),
        None,
        2.0,
    );

    let _ = detector.detect(&frame, &rois, Some(20), 3.0);
    let _ = detector.detect(&frame, &rois, Some(20), 3.1);
    let (state_new_res, event_new_res) = detector.detect(&frame, &rois, Some(20), 3.2);

    assert!(state_new_res.is_stable);
    assert!(
        event_new_res.is_some(),
        "새 결과창 진입 시 래치가 리셋되어 새 이벤트가 방출되어야 함"
    );
    let new_event = event_new_res.unwrap();
    assert_eq!(new_event.song_id, 20);
    assert_eq!(new_event.mode, super::Mode::B6);
    assert_eq!(new_event.diff, super::Difficulty::HD);
    assert_eq!(new_event.rate, 98.0);
}

#[test]
fn test_verified_play_event_re_emits_when_max_combo_or_rate_improves() {
    let mut detector = PlayStateDetector::new(1);
    detector.result_mode_diff.mode.update(Some(super::Mode::B4));
    detector
        .result_mode_diff
        .diff
        .update(Some(super::Difficulty::NM));

    let frame = blank_frame();
    let mut rois = RoiManager::new(1920, 1080);
    rois.set_scene(SceneType::ResultFreestyle);

    // 1. 점수 롤링 중 95.0% 감지 (MC=false)
    detector.rate_cache.set(
        Some(PatternRecord::Played {
            rate: 95.0,
            is_max_combo: false,
        }),
        None,
        0.0,
    );
    let (state1, event1) = detector.detect(&frame, &rois, Some(10), 1.0);
    assert!(state1.is_stable);
    assert!(event1.is_some());
    let e1 = event1.unwrap();
    assert_eq!(e1.rate, 95.0);
    assert!(!e1.is_max_combo);

    // 2. 동일 상태 유지 -> 이벤트 미방출
    let (_, event2) = detector.detect(&frame, &rois, Some(10), 1.1);
    assert!(event2.is_none());

    // 3. 점수 연출 완료로 99.5% 도달 (rate 향상) -> 이벤트 재방출!
    detector.rate_cache.set(
        Some(PatternRecord::Played {
            rate: 99.5,
            is_max_combo: false,
        }),
        None,
        0.0,
    );
    let (_, event3) = detector.detect(&frame, &rois, Some(10), 1.2);
    assert!(event3.is_some());
    let e3 = event3.unwrap();
    assert_eq!(e3.rate, 99.5);
    assert!(!e3.is_max_combo);

    // 4. MAX COMBO 뱃지 연출 등장 (MC: false -> true 개선) -> 이벤트 재방출!
    let stable_mc = Some(overmax_core::PlayContext {
        song_id: 10,
        mode: super::Mode::B4,
        diff: super::Difficulty::NM,
        rate: 99.5,
        is_max_combo: true,
    });
    let (_, event4) = detector.build_session_state_and_event(
        SceneType::ResultFreestyle,
        stable_mc.clone(),
        stable_mc.clone(),
    );
    assert!(event4.is_some());
    let e4 = event4.unwrap();
    assert_eq!(e4.rate, 99.5);
    assert!(e4.is_max_combo);

    // 5. MAX COMBO 상태 유지 -> 이벤트 미방출
    let (_, event5) =
        detector.build_session_state_and_event(SceneType::ResultFreestyle, None, stable_mc);
    assert!(event5.is_none());
}

#[test]
fn unplayed_song_after_recorded_song_does_not_retain_previous_rate() {
    let mut detector = PlayStateDetector::new(1);
    let mut frame = blank_frame();
    // 4B Mode Color & NM Diff Card
    paint_rect(&mut frame, 80, 130, 85, 135, Bgr::from_rgb_hex(0x2D4F55)); // 4B
    paint_rect(&mut frame, 98, 488, 208, 516, Bgr::from_rgb_hex(0xDCDCDC)); // NM
    let mut rois = RoiManager::new(1920, 1080);
    rois.set_scene(SceneType::Freestyle);
    paint_song_select_rate_zero(&mut frame); // 기록 없음 = 0.00%

    // 1. 곡 1 (기록 있음: 98.5%)
    let checksums = PlayStateDetector::rate_input_checksums(&frame, &rois);
    detector
        .rate_cache
        .sync_key(Some((1, super::Mode::B4, super::Difficulty::NM)));
    detector.rate_cache.set(
        Some(PatternRecord::Played {
            rate: 98.5,
            is_max_combo: false,
        }),
        checksums,
        1.0,
    );

    let (state1, _) = detector.detect(&frame, &rois, Some(1), 1.0);
    assert_eq!(state1.context.as_ref().unwrap().rate, 98.5);

    // 2. 곡 2 (기록 없음 0.00% / 미플레이)로 변경
    let (state2, _) = detector.detect(&frame, &rois, Some(2), 2.0);
    assert_eq!(
        state2.context.as_ref().unwrap().rate,
        0.0,
        "이전 곡의 기록(98.5%)이 새 미플레이 곡(0.00%)에 잔류하면 안 됨"
    );
    assert_eq!(detector.rate_cache.get(), Some(&PatternRecord::Unplayed));
}

#[test]
fn rate_input_change_without_valid_score_clears_cached_rate() {
    let mut detector = PlayStateDetector::new(1);
    let mut frame = blank_frame();
    paint_rect(&mut frame, 80, 130, 85, 135, Bgr::from_rgb_hex(0x2D4F55)); // 4B
    paint_rect(&mut frame, 98, 488, 208, 516, Bgr::from_rgb_hex(0xDCDCDC)); // NM
    let mut rois = RoiManager::new(1920, 1080);
    rois.set_scene(SceneType::Freestyle);

    // 임의의 이전 rate 캐시와 체크섬 설정
    detector
        .rate_cache
        .sync_key(Some((1, super::Mode::B4, super::Difficulty::NM)));
    detector.rate_cache.set(
        Some(PatternRecord::Played {
            rate: 95.0,
            is_max_combo: false,
        }),
        Some(RateInputChecksums {
            score: 100,
            badge: 0,
        }),
        0.0,
    );

    // score 영역에 체크섬 변경 유발 (하지만 빈 화면이라 유효한 점수가 검출되지 않음)
    paint_rect(
        &mut frame,
        1000,
        100,
        1050,
        120,
        Bgr::from_rgb_hex(0xFFFFFF),
    );

    let (state, _) = detector.detect(&frame, &rois, Some(1), 1.0);
    assert_eq!(
        state.context.as_ref().unwrap().rate,
        0.0,
        "유효한 점수가 없는 새로운 입력 감지 시 캐시된 점수가 0.0으로 클리어되어야 함"
    );
    // 0.00% 표시도 없으므로 미플레이로 단정하지 않고 판독 실패로 남긴다
    assert_eq!(detector.rate_cache.get(), None);
    assert!(detector.rate_unread);
}

#[test]
fn song_select_unreadable_rate_never_emits_unplayed_event() {
    let mut detector = PlayStateDetector::new(3);
    let mut frame = blank_frame();
    paint_rect(&mut frame, 80, 130, 85, 135, Bgr::from_rgb_hex(0x2D4F55)); // 4B
    paint_rect(&mut frame, 98, 488, 208, 516, Bgr::from_rgb_hex(0xDCDCDC)); // NM
    let mut rois = RoiManager::new(1920, 1080);
    rois.set_scene(SceneType::Freestyle);

    // rate 영역이 비어 있음 = 판독 실패 (기록 없음 표시 0.00%가 아님)
    for i in 0..10 {
        let (state, event) = detector.detect(&frame, &rois, Some(7), f64::from(i));
        assert!(
            event.is_none(),
            "판독 실패가 미플레이 이벤트로 확정되면 안 됨"
        );
        assert!(!state.is_stable);
        assert_eq!(state.context.as_ref().map(|c| c.rate), Some(0.0));
    }
}

#[test]
fn song_select_zero_rate_display_emits_unplayed_event() {
    let mut detector = PlayStateDetector::new(3);
    let mut frame = blank_frame();
    paint_rect(&mut frame, 80, 130, 85, 135, Bgr::from_rgb_hex(0x2D4F55)); // 4B
    paint_rect(&mut frame, 98, 488, 208, 516, Bgr::from_rgb_hex(0xDCDCDC)); // NM
    paint_song_select_rate_zero(&mut frame);
    let mut rois = RoiManager::new(1920, 1080);
    rois.set_scene(SceneType::Freestyle);

    let events: Vec<_> = (0..10)
        .filter_map(|i| detector.detect(&frame, &rois, Some(7), f64::from(i)).1)
        .collect();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].rate, 0.0);
    assert!(!events[0].is_result_screen);
}

#[test]
fn test_verified_play_event_emits_on_song_select_screen() {
    let mut detector = PlayStateDetector::new(3);
    let mut frame = blank_frame();
    paint_rect(&mut frame, 80, 130, 85, 135, Bgr::from_rgb_hex(0x2D4F55)); // 4B
    paint_rect(&mut frame, 98, 488, 208, 516, Bgr::from_rgb_hex(0xDCDCDC)); // NM
    let mut rois = RoiManager::new(1920, 1080);
    rois.set_scene(SceneType::Freestyle);

    // 곡 1에 대해 99.63% (MC=true) 점수 주입
    let checksums = PlayStateDetector::rate_input_checksums(&frame, &rois);
    detector
        .rate_cache
        .sync_key(Some((100, super::Mode::B4, super::Difficulty::NM)));
    detector.rate_cache.set(
        Some(PatternRecord::Played {
            rate: 99.63,
            is_max_combo: true,
        }),
        checksums,
        1.0,
    );

    // Frame 1, 2: 안정화 중 (히스토리 3개 필요)
    let (_, e1) = detector.detect(&frame, &rois, Some(100), 1.0);
    let (_, e2) = detector.detect(&frame, &rois, Some(100), 1.1);
    assert!(e1.is_none());
    assert!(e2.is_none());

    // Frame 3: 3연속 일치로 안정화 완료 -> 선곡 화면 1회 이벤트 방출!
    let (state3, e3) = detector.detect(&frame, &rois, Some(100), 1.2);
    assert!(state3.is_stable);
    assert!(
        e3.is_some(),
        "선곡 화면에서 첫 안정화 시 이벤트가 방출되어야 함"
    );
    let event = e3.unwrap();
    assert_eq!(event.song_id, 100);
    assert_eq!(event.mode, super::Mode::B4);
    assert_eq!(event.diff, super::Difficulty::NM);
    assert_eq!(event.rate, 99.63);
    assert!(
        event.is_max_combo,
        "주입된 맥스 콤보 상태가 정상 보존되어야 함"
    );
    assert!(
        !event.is_result_screen,
        "선곡 화면이므로 is_result_screen = false 여야 함 (무조건 Update 정책)"
    );

    // Frame 4: 동일 곡 선곡 화면 체류 -> 중복 방출 0회
    let (_, e4) = detector.detect(&frame, &rois, Some(100), 1.3);
    assert!(e4.is_none(), "동일 곡 체류 중에는 중복 방출되지 않아야 함");

    // 곡 2로 전환 및 95.0% 주입
    detector
        .rate_cache
        .sync_key(Some((200, super::Mode::B4, super::Difficulty::NM)));
    detector.rate_cache.set(
        Some(PatternRecord::Played {
            rate: 95.0,
            is_max_combo: false,
        }),
        checksums,
        2.0,
    );

    let _ = detector.detect(&frame, &rois, Some(200), 2.0);
    let _ = detector.detect(&frame, &rois, Some(200), 2.1);
    let (state2_3, e2_3) = detector.detect(&frame, &rois, Some(200), 2.2);

    assert!(state2_3.is_stable);
    assert!(e2_3.is_some(), "새 곡 전환 시 새 이벤트가 방출되어야 함");
    let event2 = e2_3.unwrap();
    assert_eq!(event2.song_id, 200);
    assert_eq!(event2.rate, 95.0);
    assert!(!event2.is_result_screen);
}
