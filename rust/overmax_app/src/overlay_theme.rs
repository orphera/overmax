use eframe::egui::Color32;

pub struct Theme;

impl Theme {
    // Backgrounds
    pub const PANEL_BG: Color32 = Color32::from_rgb(18, 24, 38);
    pub const PANEL_STROKE: Color32 = Color32::from_rgb(18, 24, 38);
    pub const HEADER_BG: Color32 = Color32::from_rgb(30, 40, 62);
    pub const SECTION_BG: Color32 = Color32::from_rgb(22, 30, 48);
    pub const ROW_BG: Color32 = Color32::from_rgb(36, 46, 70);

    // Tab Backgrounds
    pub const TAB_ACTIVE_BG: Color32 = Color32::from_rgb(63, 80, 117);
    pub const TAB_INACTIVE_BG: Color32 = Color32::from_rgb(28, 36, 54);
    pub const TAB_DIM_BG: Color32 = Color32::from_rgb(20, 26, 40);

    // Text Colors
    pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(240, 244, 255);      // #F0F4FF
    pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(136, 145, 167);   // #8891A7
    pub const TEXT_MUTED: Color32 = Color32::from_rgb(80, 88, 112);         // #505870
    pub const TEXT_ACCENT: Color32 = Color32::from_rgb(255, 209, 102);      // #FFD166
    pub const TEXT_BRIGHT: Color32 = Color32::from_rgb(232, 238, 255);      // #E8EEFF
    pub const TEXT_HINT: Color32 = Color32::from_rgb(180, 203, 255);        // #B4CBFF

    // Status Colors
    pub const OK: Color32 = Color32::from_rgb(0, 212, 255);
    pub const WARN: Color32 = Color32::from_rgb(255, 75, 75);
}
