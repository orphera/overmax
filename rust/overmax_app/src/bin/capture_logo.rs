#![allow(dead_code)]

use std::env;
use std::fs;
use std::path::PathBuf;

use overmax_engine::capture::capture_engine::{AdaptiveCaptureEngine, CaptureEngine};
use overmax_engine::capture::frame_utils::crop_roi;
use overmax_engine::capture::window_tracker::WindowTracker;
use overmax_engine::detector::roi::RoiManager;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("사용법: cargo run --bin capture_logo <freestyle|online>");
        std::process::exit(1);
    }

    let mode = args[1].to_lowercase();
    if mode != "freestyle" && mode != "online" {
        eprintln!("에러: 모드는 'freestyle' 또는 'online' 이어야 합니다.");
        std::process::exit(1);
    }

    println!("[Bootstrap] DJMAX RESPECT V 창을 찾는 중...");
    // settings.json 에 등록된 타이틀을 찾지 않고 디폴트 타이틀 "DJMAX RESPECT V"로 감시
    let tracker = WindowTracker::new("DJMAX RESPECT V");
    #[cfg(target_os = "linux")]
    let (snapshot, rect) = {
        let snapshot = match tracker.game_snapshot() {
            Ok(Some(snapshot)) => snapshot,
            Ok(None) => {
                eprintln!("에러: 게임이 실행 중이지 않거나 DJMAX RESPECT V 창을 찾을 수 없습니다.");
                std::process::exit(1);
            }
            Err(error) => {
                eprintln!("에러: 게임 창 추적 실패: {error}");
                std::process::exit(1);
            }
        };

        if !snapshot.fullscreen {
            eprintln!("에러: Linux 캡처는 borderless fullscreen 게임 창만 지원합니다.");
            std::process::exit(1);
        }
        if !snapshot.foreground {
            eprintln!("에러: 템플릿 캡처 전에 게임 창을 foreground로 전환하세요.");
            std::process::exit(1);
        }

        (snapshot, snapshot.rect)
    };
    #[cfg(target_os = "windows")]
    let Some(rect) = tracker.game_rect() else {
        eprintln!("에러: 게임이 실행 중이지 않거나 DJMAX RESPECT V 창을 찾을 수 없습니다.");
        std::process::exit(1);
    };

    println!(
        "[Bootstrap] 창 발견: {}x{} @ ({},{})",
        rect.width, rect.height, rect.left, rect.top
    );
    println!("[Bootstrap] 화면 캡처 중...");

    let mut capturer: Box<dyn CaptureEngine> = match AdaptiveCaptureEngine::new() {
        Ok(c) => Box::new(c),
        Err(e) => {
            eprintln!("에러: 캡처러 초기화 실패: {}", e);
            std::process::exit(1);
        }
    };
    #[cfg(target_os = "linux")]
    if let Err(error) = capturer.set_target(Some(snapshot)) {
        eprintln!("에러: 캡처 대상 설정 실패: {error}");
        std::process::exit(1);
    }

    let frame = match capturer.capture_bgra(rect) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("에러: 화면 캡처 실패: {}", e);
            std::process::exit(1);
        }
    };

    println!("[Bootstrap] 로고 영역 크롭 및 HOG 계산 중...");
    let roi_manager = RoiManager::new(frame.width, frame.height);
    let Some(logo_roi) = roi_manager.get_roi("logo") else {
        eprintln!("에러: 로고 ROI 정의를 가져올 수 없습니다.");
        std::process::exit(1);
    };

    let Some(logo_region) = crop_roi(&frame, logo_roi) else {
        eprintln!("에러: 로고 ROI 크롭 실패.");
        std::process::exit(1);
    };

    // HOG 특징 벡터 추출 (overmax_cv 내의 compute_image_features 호출)
    let features = match overmax_cv::compute_image_features(
        &logo_region.bgra,
        logo_region.width as usize,
        logo_region.height as usize,
        4,
    ) {
        Ok((_, _, _, hog)) => hog,
        Err(e) => {
            eprintln!("에러: HOG 연산 실패: {}", e);
            std::process::exit(1);
        }
    };

    if features.len() != 1764 {
        eprintln!(
            "에러: 추출된 HOG 크기가 1764가 아닙니다 (현재 크기: {})",
            features.len()
        );
        std::process::exit(1);
    }

    // 파일 경로 설정
    let mut target_path = PathBuf::from("rust/overmax_app/src/detector/templates/logo.rs");
    if !target_path.exists() {
        let alt = PathBuf::from("src/detector/templates/logo.rs");
        if alt.parent().map(|p| p.exists()).unwrap_or(false) {
            target_path = alt;
        } else {
            target_path = PathBuf::from("logo.rs");
        }
    }

    println!(
        "[Bootstrap] HOG 빌드 완료. 대상 파일: {}",
        target_path.display()
    );

    // 기존 파일 내용 읽기
    let mut file_content = if target_path.exists() {
        fs::read_to_string(&target_path).unwrap_or_default()
    } else {
        String::new()
    };

    if file_content.is_empty() {
        file_content = String::from("// 이 파일은 capture_logo 도구에 의해 자동 생성되었습니다.\n// 수동으로 편집하지 마십시오.\n\n");
    }

    // HOG 배열 문자열 생성
    let mut arr_str = String::from("[\n");
    for (i, val) in features.iter().enumerate() {
        arr_str.push_str(&format!("    {:.6},", val));
        if (i + 1) % 8 == 0 {
            arr_str.push('\n');
        } else {
            arr_str.push(' ');
        }
    }
    if !arr_str.ends_with('\n') {
        arr_str.push('\n');
    }
    arr_str.push(']');

    let const_name = if mode == "freestyle" {
        "TEMPLATE_FREESTYLE_HOG"
    } else {
        "TEMPLATE_ONLINE_HOG"
    };

    let new_const_decl = format!("pub const {}: [f32; 1764] = {};\n", const_name, arr_str);

    // regex 없이 문자열 함수만으로 기존 상수 교체
    let start_token = format!("pub const {}: [f32; 1764] =", const_name);
    let updated_content = if let Some(start_idx) = file_content.find(&start_token) {
        if let Some(end_offset) = file_content[start_idx..].find("];") {
            let end_idx = start_idx + end_offset + 2;
            let mut new_content = String::new();
            new_content.push_str(&file_content[..start_idx]);
            new_content.push_str(&new_const_decl);
            new_content.push_str(&file_content[end_idx..]);
            new_content
        } else {
            format!("{}{}\n", file_content, new_const_decl)
        }
    } else {
        format!("{}{}\n", file_content, new_const_decl)
    };

    if let Err(e) = fs::write(&target_path, updated_content) {
        eprintln!("%러: 파일 쓰기 실패: {}", e);
        std::process::exit(1);
    }

    println!(
        "[Bootstrap] 성공: {} 상수가 파일에 기록되었습니다.",
        const_name
    );
}
