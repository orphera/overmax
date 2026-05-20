use eframe::egui::FontData;

fn main() {
    let font_bytes = vec![0u8; 100];
    let _ = FontData::from_owned(font_bytes).index(0);
}
