use super::{DetectionPipeline, JacketMatchStatus, SleepHint};
use crate::jacket_index::{JacketHit, JacketIndex};
use overmax_frame::CapturedFrame;

/// 적재되지 않은 `ImageIndexDb`(엔트리 0개)와 같은 동작을 하는 테스트용 자켓 인덱스:
/// 매칭 결과 없음, centroid 게이트 통과, 지정한 임계값.
struct StubJacketIndex {
    ready: bool,
    threshold: f32,
}

impl StubJacketIndex {
    fn missing(threshold: f32) -> Self {
        Self {
            ready: false,
            threshold,
        }
    }
}

impl JacketIndex for StubJacketIndex {
    fn is_ready(&self) -> bool {
        self.ready
    }
    fn similarity_threshold(&self) -> f32 {
        self.threshold
    }
    fn check_centroid_kernel(&self, _: &[u8], _: usize, _: usize, _: usize) -> bool {
        true
    }
    fn match_jacket(&self, _: &[u8], _: usize, _: usize, _: usize) -> Option<JacketHit> {
        None
    }
}

fn replay_result_observations(
    pipeline: &mut DetectionPipeline<StubJacketIndex>,
    now: f64,
    rate: f32,
    is_max_combo: bool,
) -> Vec<overmax_core::VerifiedPlayEvent> {
    replay_result_observations_with_frame(pipeline, &blank_frame(), now, rate, is_max_combo)
}

fn replay_result_observations_with_frame(
    pipeline: &mut DetectionPipeline<StubJacketIndex>,
    frame: &CapturedFrame,
    now: f64,
    rate: f32,
    is_max_combo: bool,
) -> Vec<overmax_core::VerifiedPlayEvent> {
    let mut events = Vec::new();
    for tick in 0..10 {
        let scene = pipeline.commit_scene(overmax_core::SceneType::ResultFreestyle);
        if scene.is_result() {
            pipeline
                .play_state
                .seed_result_observation_for_test(rate, is_max_combo);
        }
        pipeline.current_song_id = Some(7);
        let out = pipeline.process_frame_with_scene(frame, scene, now + f64::from(tick));
        if let Some(event) = out.event {
            assert!(out.state.is_stable);
            assert_eq!(out.stable_hits, 5);
            events.push(event);
        }
    }
    events
}

#[test]
fn repeat_play_after_unknown_gameplay_emits_another_completion() {
    let frame = CapturedFrame {
        width: 2560,
        height: 1440,
        bgra: vec![0; 2560 * 1440 * 4],
    };
    assert!(!crate::templates::gameplay_scene::supports_frame(&frame));
    for resume_before_replay in [false, true] {
        for (rate, is_max_combo) in [(99.86, true), (99.5, false)] {
            let mut pipeline = DetectionPipeline::with_jacket_index(StubJacketIndex::missing(0.6));
            assert_eq!(
                replay_result_observations_with_frame(&mut pipeline, &frame, 10.0, 99.86, true)
                    .len(),
                1
            );

            if resume_before_replay {
                pipeline.suspend();
                pipeline.detect(&frame, 160.0);
                assert!(replay_result_observations_with_frame(
                    &mut pipeline,
                    &frame,
                    170.0,
                    99.86,
                    true
                )
                .is_empty());
                assert_eq!(pipeline.play_state.stable_hits(), 5);
            }

            // F5 from a result skips selection. Unsupported gameplay stays
            // Unknown, but sustained misses still close the previous play.
            for now in [200.0, 202.0, 204.0, 206.0] {
                let output = pipeline.detect(&frame, now);
                assert_eq!(pipeline.last_scene, overmax_core::SceneType::Unknown);
                assert!(output.event.is_none());
            }
            assert!(!pipeline.hysteresis.is_active);
            assert_eq!(pipeline.play_state.stable_hits(), 0);

            let events = replay_result_observations_with_frame(
                &mut pipeline,
                &frame,
                370.0,
                rate,
                is_max_combo,
            );
            assert_eq!(
                events.len(),
                1,
                "a repeat play must emit even without an improvement (resumed: {resume_before_replay})"
            );
            assert_eq!(events[0].rate, rate);
            assert_eq!(events[0].is_max_combo, is_max_combo);
        }
    }
}

#[test]
fn result_resume_revalidates_without_emitting_a_second_completion() {
    let mut pipeline = DetectionPipeline::with_jacket_index(StubJacketIndex::missing(0.6));
    assert_eq!(
        replay_result_observations(&mut pipeline, 10.0, 99.86, true).len(),
        1
    );

    pipeline.suspend();
    assert_eq!(pipeline.play_state.stable_hits(), 0);
    assert!(pipeline.play_state.detected_cache_is_empty_for_test());
    assert!(pipeline.current_song_id.is_none());
    let unknown = pipeline.detect(&blank_frame(), 160.0);
    assert!(!unknown.state.is_stable);
    assert!(unknown.state.context.is_none());
    assert!(unknown.event.is_none());

    let events = replay_result_observations(&mut pipeline, 170.0, 99.86, true);
    assert!(
        events.is_empty(),
        "the same result after Alt-Tab is not a second play"
    );
    assert_eq!(pipeline.play_state.stable_hits(), 5);
}

#[test]
fn result_completion_rearms_after_a_new_play_or_target_reset() {
    use overmax_core::SceneType;
    for boundary in [
        SceneType::Gameplay,
        SceneType::Freestyle,
        SceneType::Unknown,
    ] {
        let mut pipeline = DetectionPipeline::with_jacket_index(StubJacketIndex::missing(0.6));
        assert_eq!(
            replay_result_observations(&mut pipeline, 10.0, 99.86, true).len(),
            1
        );
        pipeline.suspend();
        if boundary == SceneType::Unknown {
            // A new capture session/target invalidates the completed event too.
            pipeline.reset();
        } else {
            for tick in 0..4 {
                let scene = pipeline.commit_scene(boundary);
                pipeline.process_frame_with_scene(&blank_frame(), scene, 160.0 + f64::from(tick));
            }
        }
        assert_eq!(
            replay_result_observations(&mut pipeline, 170.0, 99.86, true).len(),
            1,
            "the same score from a new play/target must still be recorded: {boundary:?}"
        );
    }
}

#[test]
fn result_resume_still_emits_verified_rate_and_max_combo_improvements() {
    let mut pipeline = DetectionPipeline::with_jacket_index(StubJacketIndex::missing(0.6));
    assert_eq!(
        replay_result_observations(&mut pipeline, 10.0, 99.5, false).len(),
        1
    );
    for (now, rate, is_max_combo) in [(170.0, 99.86, false), (330.0, 99.86, true)] {
        pipeline.suspend();
        pipeline.detect(&blank_frame(), now - 10.0);
        let events = replay_result_observations(&mut pipeline, now, rate, is_max_combo);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].rate, rate);
        assert_eq!(events[0].is_max_combo, is_max_combo);
    }
}

#[test]
fn replace_image_db_keeps_recognition_history() {
    let mut pipeline = DetectionPipeline::with_jacket_index(StubJacketIndex::missing(0.6));
    pipeline.current_song_id = Some(42);
    pipeline.last_scene = overmax_core::SceneType::Gameplay;

    pipeline.replace_jacket_index(StubJacketIndex::missing(0.7));

    assert_eq!(pipeline.current_song_id, Some(42));
    assert_eq!(pipeline.last_scene, overmax_core::SceneType::Gameplay);
}

#[test]
fn ingame_scenes_share_result_commitment_and_break_on_misses() {
    use overmax_core::SceneType;
    let mut pipeline = DetectionPipeline::with_jacket_index(StubJacketIndex::missing(0.6));
    assert_eq!(
        pipeline.commit_scene(SceneType::Gameplay),
        SceneType::Unknown
    );
    assert_eq!(
        pipeline.commit_scene(SceneType::Gameplay),
        SceneType::Gameplay
    );
    assert_eq!(pipeline.commit_scene(SceneType::Paused), SceneType::Unknown);
    assert_eq!(
        pipeline.commit_scene(SceneType::Unknown),
        SceneType::Unknown
    );
    assert_eq!(pipeline.commit_scene(SceneType::Paused), SceneType::Unknown);
    assert_eq!(pipeline.commit_scene(SceneType::Paused), SceneType::Paused);
    pipeline.reset();
    assert_eq!(pipeline.commit_scene(SceneType::Paused), SceneType::Unknown);
}

#[test]
fn unsupported_format_switch_invalidates_ingame_history_even_on_cached_tick() {
    use overmax_core::SceneType;
    let mut pipeline = DetectionPipeline::with_jacket_index(StubJacketIndex::missing(0.6));
    pipeline.commit_scene(SceneType::Gameplay);
    pipeline.commit_scene(SceneType::Gameplay);
    pipeline.last_scene_check_ts = 10.0;
    let unsupported = CapturedFrame {
        width: 1280,
        height: 720,
        bgra: vec![0; 1280 * 720 * 4],
    };
    let output = pipeline.detect(&unsupported, 10.01);
    assert_eq!(output.state.scene, SceneType::Unknown);
    assert_eq!(pipeline.scene_streak, 0);
    assert_eq!(
        pipeline.commit_scene(SceneType::Gameplay),
        SceneType::Unknown
    );
}

#[test]
fn incomplete_capture_clears_scene_before_any_pixel_reader_runs() {
    use overmax_core::SceneType;
    let mut pipeline = DetectionPipeline::with_jacket_index(StubJacketIndex::missing(0.6));
    pipeline.commit_scene(SceneType::Gameplay);
    pipeline.commit_scene(SceneType::Gameplay);
    let frame = CapturedFrame {
        width: 1920,
        height: 1080,
        bgra: vec![0; 16],
    };
    let output = pipeline.detect(&frame, 100.0);
    assert_eq!(output.state.scene, SceneType::Unknown);
    assert_eq!(output.event, None);
    assert_eq!(pipeline.scene_streak, 0);
}

#[test]
fn ingame_candidate_closes_record_path_before_scene_is_confirmed() {
    use overmax_core::SceneType;
    let mut pipeline = DetectionPipeline::with_jacket_index(StubJacketIndex::missing(0.6));
    let frame = blank_frame();
    pipeline.last_scene = SceneType::ResultFreestyle;
    pipeline.rois.set_scene(SceneType::ResultFreestyle);
    pipeline.hysteresis.update(true);
    pipeline.hysteresis.update(true);
    pipeline.current_song_id = Some(42);
    let scene = pipeline.commit_scene(SceneType::Gameplay);
    let first = pipeline.process_frame_with_scene(&frame, scene, 10.0);
    assert_eq!(first.state.scene, SceneType::Unknown);
    assert!(!first.is_song_select);
    assert!(!first.is_result);
    assert_eq!(first.current_song_id, None);
    assert_eq!(first.event, None);
    assert_eq!(first.jacket_status, JacketMatchStatus::NotSongSelect);

    // Cached ticks cannot confirm the pending candidate.
    let cached = pipeline.process_frame_cached(&frame, 10.1);
    assert_eq!(cached.state.scene, SceneType::Unknown);
    assert_eq!(pipeline.scene_streak, 1);
    let scene = pipeline.commit_scene(SceneType::Gameplay);
    let confirmed = pipeline.process_frame_with_scene(&frame, scene, 11.5);
    assert_eq!(confirmed.state.scene, SceneType::Gameplay);
    assert!(confirmed.scene_detected);
    assert!(!confirmed.is_song_select);
    assert!(!confirmed.state.is_stable);
    assert_eq!(confirmed.state.context, None);
    assert_eq!(confirmed.event, None);
    // Preserve relaxed capture scheduling during sustained play.
    assert_eq!(
        pipeline.process_frame_cached(&frame, 14.0).sleep_hint,
        SleepHint::Relaxed
    );
    let scene = pipeline.commit_scene(SceneType::Freestyle);
    pipeline.process_frame_with_scene(&frame, scene, 15.0);
    let recovered = pipeline.process_frame_with_scene(&frame, scene, 15.3);
    assert!(recovered.is_song_select);
}

#[test]
fn ingame_confirmation_uses_next_capture_then_restores_relaxed_polling() {
    use overmax_core::SceneType;
    let mut pipeline = DetectionPipeline::with_jacket_index(StubJacketIndex::missing(0.6));
    let mut gameplay = blank_frame();
    for y in (80..=336).step_by(32) {
        for x in [705, 1214] {
            gameplay.bgra[(y * 1920 + x) * 4] = 255;
        }
    }
    let mut paused = blank_frame();
    let title = crate::templates::gameplay_scene::PAUSE_TITLE[0];
    for (i, &gray) in title.iter().enumerate() {
        let offset = ((177 + i / 148) * 1920 + 731 + i % 148) * 4;
        paused.bgra[offset..offset + 3].fill(gray);
    }
    pipeline.unknown_since = Some(0.0);
    for (frame, start, expected) in [
        (&gameplay, 10.0, SceneType::Gameplay),
        (&paused, 14.0, SceneType::Paused),
        (&gameplay, 18.0, SceneType::Gameplay),
    ] {
        let first = pipeline.detect(frame, start);
        assert_eq!(first.state.scene, SceneType::Unknown);
        assert_eq!(first.sleep_hint, SleepHint::Relaxed);
        assert_eq!(first.state.context, None);
        assert_eq!(first.event, None);

        let cached = pipeline.detect(frame, start + 0.99);
        assert_eq!(cached.state.scene, SceneType::Unknown);
        assert_eq!(pipeline.scene_streak, 1);
        assert_eq!(pipeline.last_scene_check_ts, start);

        let confirmed = pipeline.detect(frame, start + 1.0);
        assert_eq!(confirmed.state.scene, expected);
        assert_eq!(confirmed.sleep_hint, SleepHint::Relaxed);
        assert!(!confirmed.is_song_select);
        assert!(!confirmed.state.is_stable);
        assert_eq!(confirmed.state.context, None);
        assert_eq!(confirmed.event, None);

        assert_eq!(pipeline.detect(frame, start + 2.0).state.scene, expected);
        assert_eq!(pipeline.last_scene_check_ts, start + 1.0);
        assert_eq!(pipeline.detect(frame, start + 2.5).state.scene, expected);
        assert_eq!(pipeline.last_scene_check_ts, start + 2.5);
    }
}

#[test]
fn failed_confirmation_clears_candidate_and_result_timing_stays_unchanged() {
    use overmax_core::SceneType;
    let mut pipeline = DetectionPipeline::with_jacket_index(StubJacketIndex::missing(0.6));
    let frame = blank_frame();
    pipeline.unknown_since = Some(0.0);
    pipeline.commit_scene(SceneType::Gameplay);
    pipeline.last_scene_check_ts = 10.0;
    assert_eq!(
        pipeline.detect(&frame, 11.0).state.scene,
        SceneType::Unknown
    );
    assert_eq!(pipeline.scene_streak, 0);
    assert_eq!(pipeline.pending_scene, SceneType::Unknown);
    pipeline.detect(&frame, 12.0);
    assert_eq!(pipeline.last_scene_check_ts, 11.0);
    pipeline.detect(&frame, 12.5);
    assert_eq!(pipeline.last_scene_check_ts, 12.5);

    pipeline.commit_scene(SceneType::ResultFreestyle);
    pipeline.detect(&frame, 13.5);
    assert_eq!(pipeline.last_scene_check_ts, 12.5);
    assert_eq!(pipeline.scene_streak, 1);
    pipeline.detect(&frame, 14.0);
    assert_eq!(pipeline.last_scene_check_ts, 14.0);
    assert_eq!(pipeline.scene_streak, 0);

    pipeline.reset();
    pipeline.unknown_since = Some(20.0);
    pipeline.commit_scene(SceneType::Gameplay);
    pipeline.last_scene_check_ts = 20.0;
    pipeline.detect(&frame, 20.31);
    assert_eq!(pipeline.last_scene_check_ts, 20.31);
    assert_eq!(pipeline.scene_streak, 0);
}

#[test]
fn scene_poll_miss_flips_to_unknown_and_records_diag() {
    let mut pipeline = DetectionPipeline::with_jacket_index(StubJacketIndex::missing(0.6));
    let frame = blank_frame();
    use overmax_core::SceneType;

    pipeline.last_scene = SceneType::Freestyle;
    pipeline.hysteresis.is_active = true;
    pipeline.rois.set_scene(SceneType::Freestyle);

    // 파싱 실패 시 Unknown 전환 + 진단 지표(거절 단계) 집계 확인
    let out = pipeline.detect(&frame, 10.0);
    assert_eq!(pipeline.last_scene, SceneType::Unknown);
    assert_eq!(out.sleep_hint, SleepHint::Active);

    let snap = pipeline.stats.maybe_take_snapshot(0.0).unwrap();
    assert_eq!(snap.scene_poll_misses, 1);
    assert!(snap.scene_miss_band_rejects > 0 || snap.scene_miss_centroid_rejects > 0);
}

#[test]
fn sleep_hint_is_active_during_unknown_warmup_and_relaxed_after() {
    let mut pipeline = DetectionPipeline::with_jacket_index(StubJacketIndex::missing(0.6));
    let frame = blank_frame();
    use overmax_core::SceneType;

    // Unknown 진입 초기(<3s): 고속 폴링 유지로 씬 전환 반응성 확보
    let warmup = pipeline.process_frame_with_scene(&frame, SceneType::Unknown, 1.0);
    assert_eq!(warmup.sleep_hint, SleepHint::Active);
    let late_warmup = pipeline.process_frame_with_scene(&frame, SceneType::Unknown, 2.9);
    assert_eq!(late_warmup.sleep_hint, SleepHint::Active);

    // 장기 Unknown(>=3s): 완화 주기 전환으로 캡처 비용 절감
    let relaxed = pipeline.process_frame_with_scene(&frame, SceneType::Unknown, 4.0);
    assert_eq!(relaxed.sleep_hint, SleepHint::Relaxed);

    // 씬 신호 복귀 시 즉시 Active 복원
    let recovered = pipeline.process_frame_with_scene(&frame, SceneType::Freestyle, 5.0);
    assert_eq!(recovered.sleep_hint, SleepHint::Active);
}

#[test]
fn stays_detecting_until_hysteresis_activates() {
    let mut pipeline = DetectionPipeline::with_jacket_index(StubJacketIndex::missing(0.6));
    let frame = blank_frame();
    use overmax_core::SceneType;

    let first = pipeline.process_frame_with_scene(&frame, SceneType::Freestyle, 1.0);
    let second = pipeline.process_frame_with_scene(&frame, SceneType::Freestyle, 2.0);

    assert!(!first.is_song_select);
    assert_eq!(first.jacket_status, JacketMatchStatus::NotSongSelect);
    assert!(second.is_song_select);
    assert_eq!(second.jacket_status, JacketMatchStatus::DbNotReady);
}

#[test]
fn resets_state_when_song_select_is_lost() {
    let mut pipeline = DetectionPipeline::with_jacket_index(StubJacketIndex::missing(0.6));
    let frame = blank_frame();
    use overmax_core::SceneType;

    for idx in 0..2 {
        pipeline.process_frame_with_scene(&frame, SceneType::Freestyle, idx as f64);
    }
    let output = pipeline.process_frame_with_scene(&frame, SceneType::Unknown, 10.0);

    assert!(output.is_song_select);
    assert!(output.state.context.is_none());
}

#[test]
fn cached_frames_do_not_repeat_a_scene_miss() {
    let mut pipeline = DetectionPipeline::with_jacket_index(StubJacketIndex::missing(0.6));
    let frame = blank_frame();
    use overmax_core::SceneType;

    pipeline.process_frame_with_scene(&frame, SceneType::Freestyle, 1.0);
    pipeline.process_frame_with_scene(&frame, SceneType::Freestyle, 2.0);
    let miss = pipeline.process_frame_with_scene(&frame, SceneType::Unknown, 3.0);
    assert!(miss.is_song_select);
    assert!(!miss.is_leaving);

    for now in [3.1, 3.2, 3.3, 3.4] {
        let cached = pipeline.process_frame_cached(&frame, now);
        assert!(cached.is_song_select);
        assert!(!cached.is_leaving);
    }
}

#[test]
fn cached_frames_do_not_complete_scene_entry() {
    let mut pipeline = DetectionPipeline::with_jacket_index(StubJacketIndex::missing(0.6));
    let frame = blank_frame();
    use overmax_core::SceneType;

    let fresh = pipeline.process_frame_with_scene(&frame, SceneType::Freestyle, 1.0);
    pipeline.last_scene = SceneType::Freestyle;
    let cached = pipeline.process_frame_cached(&frame, 1.1);
    let second_fresh = pipeline.process_frame_with_scene(&frame, SceneType::Freestyle, 1.2);

    assert!(!fresh.is_song_select);
    assert!(!cached.is_song_select);
    assert!(second_fresh.is_song_select);
}

#[test]
fn reset_clears_result_cache_and_roi_scene() {
    use overmax_core::SceneType;

    let mut pipeline = DetectionPipeline::with_jacket_index(StubJacketIndex::missing(0.6));
    pipeline.rois.set_scene(SceneType::ResultFreestyle);
    pipeline.play_state.seed_detected_cache_for_test();

    pipeline.reset();

    assert_eq!(pipeline.rois.current_scene(), SceneType::Unknown);
    assert!(pipeline.play_state.detected_cache_is_empty_for_test());
}

#[test]
fn song_select_emits_verified_event_once_across_stable_frames() {
    use overmax_core::SceneType;
    use overmax_frame::RoiRect;

    let mut pipeline = DetectionPipeline::with_jacket_index(StubJacketIndex::missing(0.6));
    let mut frame = blank_frame();
    let mut paint = |r: RoiRect, bgr: [u8; 3]| {
        for y in r.y1..r.y2 {
            for x in r.x1..r.x2 {
                let i = ((y * frame.width + x) * 4) as usize;
                frame.bgra[i..i + 3].copy_from_slice(&bgr);
                frame.bgra[i + 3] = 255;
            }
        }
    };
    paint(
        RoiRect {
            x1: 80,
            y1: 130,
            x2: 85,
            y2: 135,
        },
        [0x55, 0x4F, 0x2D],
    ); // 4B
    paint(
        RoiRect {
            x1: 98,
            y1: 488,
            x2: 208,
            y2: 516,
        },
        [0xDC, 0xDC, 0xDC],
    ); // NORMAL
    crate::play_state::paint_song_select_rate_zero(&mut frame); // 기록 없음 = 0.00%

    let mut stable = 0;
    let mut events = 0;
    for i in 0..30 {
        // 이미지 DB가 없으므로 곡 ID는 매 프레임 주입 (화면 이탈 시 초기화되기 때문)
        pipeline.current_song_id = Some(7);
        let out = pipeline.process_frame_with_scene(
            &frame,
            SceneType::Freestyle,
            10.0 + f64::from(i) * 0.1,
        );
        stable += usize::from(out.state.is_stable);
        events += usize::from(out.event.is_some());
    }
    assert!(stable > 1, "선곡 화면이 안정화되어야 검증 가능");
    assert_eq!(events, 1, "안정 상태에서 동일 패턴 이벤트가 중복 방출됨");
}

fn blank_frame() -> CapturedFrame {
    CapturedFrame {
        width: 1920,
        height: 1080,
        bgra: vec![0; 1920 * 1080 * 4],
    }
}

#[test]
#[ignore]
fn test_scratch_images() {
    use image::GenericImageView;
    use overmax_core::SceneType;
    use overmax_frame::crop_roi;

    let scratch_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scratch");
    let images = [
        "hd_test_1.png",
        "hd_test_2.png",
        "hd_test_3.png",
        "hd_test_4.png",
        "hd_test_5.png",
        "hd_test_2p_1.png",
        "hd_test_2p_2.png",
    ];

    let db_path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cache/image_index.db");

    let roi_dir = scratch_dir.join("roi");
    std::fs::create_dir_all(&roi_dir).unwrap();

    for img_name in &images {
        let path = scratch_dir.join(img_name);
        if !path.exists() {
            println!("{}: Not found", img_name);
            continue;
        }

        // Create a fresh pipeline for each image to isolate ROI checksum caches (mode/diff, rate)
        // 기존 동작 유지: 매처는 적재 전 DB로 만들어져 비어 있고, 적재 성공 시 is_ready만 참이 된다.
        let mut pipeline = DetectionPipeline::with_jacket_index(StubJacketIndex {
            ready: db_path.exists(),
            threshold: 0.6,
        });

        let img = image::ImageReader::open(&path)
            .expect("Failed to open file")
            .with_guessed_format()
            .expect("Failed to guess format")
            .decode()
            .expect("Failed to decode image");
        let (w, h) = img.dimensions();
        let mut bgra = vec![0u8; (w * h * 4) as usize];

        for (x, y, pixel) in img.pixels() {
            let idx = ((y * w + x) * 4) as usize;
            overmax_cv::Bgr::new(pixel[2], pixel[1], pixel[0])
                .write_to_bgra(&mut bgra[idx..idx + 4], pixel[3]);
        }

        let frame = CapturedFrame {
            width: w as i32,
            height: h as i32,
            bgra,
        };

        pipeline.rois.update_window_size(w as i32, h as i32);
        pipeline.reset_on_screen_exit();

        // 1. Detect final stable scene
        let mut final_scene = SceneType::Unknown;
        for step in 0..10 {
            let t = step as f64 * 0.4;
            let _ = pipeline.detect(&frame, t);
            final_scene = pipeline.last_scene;
        }
        println!("IMAGE: {} -> Detected Scene: {:?}", img_name, final_scene);

        // 2. Build ROI name list for current scene
        let mut roi_names = Vec::new();
        if let Some(cfg) = pipeline.rois.config.scenes.get(&final_scene) {
            for name in cfg.rois.keys() {
                roi_names.push(name.clone());
            }
        }
        if final_scene == SceneType::Freestyle
            || final_scene == SceneType::OpenMatch
            || final_scene == SceneType::LadderMatch
        {
            for diff in overmax_core::Difficulty::ALL {
                roi_names.push(format!("diff_panel_{}", diff.as_str()));
            }
        }

        // 3. Crop and save each ROI
        for roi_name in roi_names {
            let roi_rect = if roi_name.starts_with("diff_panel_") {
                let diff_name = roi_name.strip_prefix("diff_panel_").unwrap();
                overmax_core::Difficulty::from_str(diff_name).and_then(|diff| {
                    pipeline
                        .rois
                        .get_diff_panel_roi_for_scene(diff, final_scene)
                })
            } else {
                pipeline.rois.get_roi_for_scene(&roi_name, final_scene)
            };

            let Some(roi) = roi_rect else {
                continue;
            };

            let Some(cropped) = crop_roi(&frame, roi) else {
                continue;
            };

            let mut rgba = cropped.to_image_region().bgra;
            for chunk in rgba.chunks_exact_mut(4) {
                chunk.swap(0, 2); // BGR -> RGB
            }

            let out_filename = format!(
                "{}_{}.png",
                img_name.strip_suffix(".png").unwrap_or(img_name),
                roi_name
            );
            let out_path = roi_dir.join(out_filename);
            image::save_buffer(
                &out_path,
                &rgba,
                cropped.width as u32,
                cropped.height as u32,
                image::ColorType::Rgba8,
            )
            .expect("Failed to save cropped image");
            println!("    Saved ROI '{}' to {:?}", roi_name, out_path);
        }
    }
}

#[test]
fn gameplay_atlas_and_full_frame_return_same_candidate_for_equivalent_evidence() {
    use overmax_core::SceneType;
    let mut rois = crate::roi::RoiManager::new(1920, 1080);
    let mut full = CapturedFrame {
        width: 1920,
        height: 1080,
        bgra: vec![0; 1920 * 1080 * 4],
    };
    for y in (80..=336).step_by(32) {
        for x in [705, 1214] {
            full.bgra[(y * 1920 + x) * 4] = 255;
        }
    }
    assert_eq!(
        crate::templates::gameplay_scene::read_scene(&full, &rois),
        SceneType::Gameplay
    );
    let atlas = CapturedFrame {
        width: 512,
        height: 512,
        bgra: vec![0; 512 * 512 * 4],
    };
    rois.update_window_size(atlas.width, atlas.height);
    assert!(crate::templates::gameplay_scene::supports_frame(&atlas));
    assert_eq!(
        crate::templates::gameplay_scene::read_scene(&atlas, &rois),
        SceneType::Unknown
    );
}

/// 오픈매치 결과창 2인/3인 판별(`check_open_match_badge`)은 player_panel 바깥 8px 마진의
/// 엣지를 읽는다. 아틀라스 모드에서도 그 마진은 이웃 슬롯이 아닌 실제 화면 픽셀이어야 한다.
#[test]
fn open_match_player_panel_edge_margin_is_identical_in_atlas() {
    use crate::roi::RoiManager;
    use overmax_core::SceneType;
    use overmax_frame::build_virtual_atlas;
    use overmax_frame::crop_roi;

    let (w, h) = (1920usize, 1080usize);
    let mut bgra = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            bgra.extend_from_slice(&[
                (x & 0xFF) as u8,
                (y & 0xFF) as u8,
                ((x ^ y) & 0xFF) as u8,
                255,
            ]);
        }
    }
    let full = CapturedFrame {
        width: w as i32,
        height: h as i32,
        bgra,
    };
    let atlas = build_virtual_atlas(&full);
    let full_rois = RoiManager::new(full.width, full.height);
    let atlas_rois = RoiManager::new(atlas.width, atlas.height);

    for scene in [SceneType::ResultOpen3, SceneType::ResultOpen2] {
        let full_roi = full_rois
            .get_roi_for_scene("player_panel", scene)
            .unwrap()
            .with_margin(8);
        let atlas_roi = atlas_rois
            .get_roi_for_scene("player_panel", scene)
            .unwrap()
            .with_margin(8);
        let full_crop = crop_roi(&full, full_roi).unwrap();
        let atlas_crop = crop_roi(&atlas, atlas_roi).unwrap();
        assert_eq!(
            (full_crop.width, full_crop.height),
            (atlas_crop.width, atlas_crop.height),
            "{scene:?} player_panel margin crop size mismatch"
        );
        for y in 0..full_crop.height {
            assert_eq!(
                full_crop.row(y),
                atlas_crop.row(y),
                "{scene:?} player_panel margin pixel mismatch at row {y}"
            );
        }
    }
}

#[test]
fn cached_tick_with_unsupported_frame_resets_ingame_to_unknown() {
    use overmax_core::SceneType;
    let mut pipeline = DetectionPipeline::with_jacket_index(StubJacketIndex::missing(0.6));
    pipeline.commit_scene(SceneType::Gameplay);
    pipeline.commit_scene(SceneType::Gameplay);
    pipeline.last_scene_check_ts = 10.0;
    let unsupported = CapturedFrame {
        width: 1280,
        height: 720,
        bgra: vec![0; 1280 * 720 * 4],
    };
    let output = pipeline.detect(&unsupported, 10.01);
    assert_eq!(output.state.scene, SceneType::Unknown);
    assert_eq!(pipeline.scene_streak, 0);
    assert_eq!(
        pipeline.commit_scene(SceneType::Gameplay),
        SceneType::Unknown
    );
}
