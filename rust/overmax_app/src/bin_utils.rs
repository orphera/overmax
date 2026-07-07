use std::path::Path;
use image::GenericImageView;
use overmax_engine::capture::frame::CapturedFrame;

pub fn load_frame(path: &Path) -> Option<CapturedFrame> {
    let img = match image::open(path) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("      [Error] Failed to open image '{}': {}", path.display(), e);
            return None;
        }
    };
    let (w, h) = img.dimensions();
    println!("      [Image Resolution] Original size: {}x{}", w, h);
    // 1920x1080 리사이즈 (hd_* 파일들의 해상도가 FHD가 아닐 경우 대비)
    let img_resized = if w != 1920 || h != 1080 {
        img.resize_exact(1920, 1080, image::imageops::FilterType::Lanczos3)
    } else {
        img
    };
    
    let mut rgba = img_resized.to_rgba8().into_raw();
    for chunk in rgba.chunks_exact_mut(4) {
        chunk.swap(0, 2);
    }
    Some(CapturedFrame {
        width: 1920,
        height: 1080,
        bgra: rgba,
    })
}
