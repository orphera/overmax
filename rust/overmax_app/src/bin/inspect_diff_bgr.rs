use std::path::Path;
use image::GenericImageView;

/// 결과 화면 전용 난이도 패널 템플릿 생성기 (유저 Ground Truth 기준 + 동적 임계값 이진화)
/// ROI: (709, 86, 90, 18) - x_start, y_start, width, height (FHD 1920x1080 기준)
fn main() {
    let samples = vec![
        ("NM", "scratch/screenshots/20260702233605_1.jpg"),
        ("HD", "scratch/screenshots/20260702233310_1.jpg"),
        ("MX", "scratch/screenshots/20260702233900_1.jpg"),
        ("SC", "scratch/screenshots/20260703020235_1.jpg"),
    ];

    let roi_x = 709;
    let roi_y = 86;
    let roi_w = 90;
    let roi_h = 18;

    println!("=== Result Screen Difficulty Template Generator ===");
    println!("ROI: x={}, y={}, w={}, h={}", roi_x, roi_y, roi_w, roi_h);
    println!();

    for (label, path) in &samples {
        let img_path = Path::new(path);
        if !img_path.exists() {
            println!("[{}] File not found: {}", label, path);
            continue;
        }

        let img = image::open(img_path).expect("failed to open image");
        let (w, h) = img.dimensions();
        let img_resized = if w != 1920 || h != 1080 {
            img.resize_exact(1920, 1080, image::imageops::FilterType::Lanczos3)
        } else {
            img
        };

        // ROI 크롭
        let cropped = img_resized.crop_imm(roi_x, roi_y, roi_w, roi_h);
        let crop_path = format!("scratch/screenshots/result_diff_panel_{}.png", label);
        cropped.save(&crop_path).ok();

        // 동적 임계값 구하기
        let gray = cropped.to_luma8();
        let mut max_y = 0u8;
        let mut min_y = 255u8;
        for y in 0..roi_h {
            for x in 0..roi_w {
                let v = gray.get_pixel(x, y)[0];
                if v > max_y { max_y = v; }
                if v < min_y { min_y = v; }
            }
        }

        // 임계값 = (min_y + max_y) / 2 또는 max_y * 0.75 등의 동적 결정
        // NORMAL, HARD, MAXIMUM, SC 의 글자 색상이 배경색 대비 밝으므로, 적정 지점을 찾음
        let threshold = overmax_cv::diff_panel_threshold(max_y, min_y);

        println!("[{}] Path: {}, Min Luma: {}, Max Luma: {}, Auto Threshold: {}", label, path, min_y, max_y, threshold);

        // 이진화 저장
        let mut binary = image::GrayImage::new(roi_w, roi_h);
        for y in 0..roi_h {
            for x in 0..roi_w {
                let v = gray.get_pixel(x, y)[0];
                binary.put_pixel(x, y, image::Luma([if v >= threshold { 255 } else { 0 }]));
            }
        }
        let bin_path = format!("scratch/screenshots/result_diff_panel_{}_bin.png", label);
        binary.save(&bin_path).ok();

        // 마스크 배열 출력
        println!("[{}] Mask ({}x{}):", label, roi_w, roi_h);
        println!("const RESULT_DIFF_MASK_{}: [u8; {}] = [", label, roi_w as usize * roi_h as usize);
        for y in 0..roi_h {
            print!("    ");
            for x in 0..roi_w {
                let v = if gray.get_pixel(x, y)[0] >= threshold { 1 } else { 0 };
                print!("{}, ", v);
            }
            println!();
        }
        println!("];");
        println!();
    }
}
