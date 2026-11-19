//! Debug log ring buffer and deferred viewport content.

use eframe::egui::{
    self, Color32, CornerRadius, Frame, Margin, RichText, ScrollArea, Stroke, ViewportClass,
};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::ui::overlay_theme::{apply_secondary_window_style, Theme};

#[derive(Clone, Debug, Default)]
pub struct DebugAppStateSnapshot {
    pub scene_label: String,
    pub confidence: f32,
    pub game_found: bool,
    pub is_active: bool,
    pub overlay_on: bool,
    pub always_visible: bool,
    pub opacity: f32,
    pub capture_engine: String,
    pub content_protected: bool,
    pub cached_hwnd: Option<isize>,
    pub game_hwnd: Option<isize>,
    pub song_info: String,
    pub play_state_info: String,
    pub jacket_match_info: String,
    pub capture_res_info: String,
    pub top_jacket_similarity: Option<f32>,
    pub roi_scale: f32,
    pub roi_offset_y: i32,
    pub stable_hits: u32,
    pub telemetry_snapshot: Option<overmax_detect::PipelineTelemetrySnapshot>,
}

pub fn push_log(lines: &Arc<Mutex<VecDeque<Arc<str>>>>, max_lines: usize, line: impl AsRef<str>) {
    let Ok(mut g) = lines.lock() else {
        return;
    };
    if g.len() >= max_lines {
        g.pop_front();
    }
    g.push_back(Arc::from(line.as_ref()));
}

pub fn close_if_requested(ctx: &egui::Context, open: &Arc<AtomicBool>) {
    if ctx.input(|i| i.viewport().close_requested() || i.key_pressed(egui::Key::Escape)) {
        open.store(false, Ordering::Relaxed);
    }
}

pub fn render_debug(
    ui: &mut egui::Ui,
    class: ViewportClass,
    title: &str,
    lines: &Arc<Mutex<VecDeque<Arc<str>>>>,
    paused: &Arc<AtomicBool>,
    filters: &Arc<Mutex<std::collections::HashMap<String, bool>>>,
    app_state: &DebugAppStateSnapshot,
) {
    apply_secondary_window_style(ui.ctx());

    if class == ViewportClass::EmbeddedWindow {
        egui::Window::new(title).show(ui.ctx(), |ui| {
            render_app_state_dashboard(ui, app_state);
            ui.add_space(8.0);
            render_controls(ui, lines, paused, filters);
            ui.add_space(8.0);
            log_scroll(ui, lines, filters);
        });
    } else {
        egui::CentralPanel::default()
            .frame(
                Frame::new()
                    .fill(Theme::PANEL_BG)
                    .inner_margin(Margin::same(20)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    ui.label(
                        RichText::new("Overmax")
                            .color(Theme::TEXT_ACCENT)
                            .size(Theme::FONT_HEADING)
                            .strong(),
                    );
                    ui.label(
                        RichText::new("Debug Telemetry")
                            .color(Theme::TEXT_PRIMARY)
                            .size(Theme::FONT_HEADING)
                            .strong(),
                    );

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let total_lines = if let Ok(g) = lines.lock() { g.len() } else { 0 };
                        ui.label(
                            RichText::new(format!("{} lines", total_lines))
                                .color(Theme::TEXT_MUTED)
                                .size(Theme::FONT_TINY),
                        );
                    });
                });
                ui.add_space(14.0);

                render_app_state_dashboard(ui, app_state);
                ui.add_space(14.0);

                render_controls(ui, lines, paused, filters);
                ui.add_space(12.0);

                log_scroll(ui, lines, filters);
            });
    }
}

fn render_app_state_dashboard(ui: &mut egui::Ui, state: &DebugAppStateSnapshot) {
    Frame::new()
        .fill(Theme::CARD)
        .stroke(Stroke::new(1.0_f32, Theme::PANEL_STROKE))
        .corner_radius(CornerRadius::same(Theme::R_MD))
        .inner_margin(Margin::same(14))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.vertical(|ui| {
                ui.columns(3, |cols| {
                    // Col 1: Scene & Confidence
                    cols[0].vertical(|ui| {
                        ui.label(
                            RichText::new("SCENE / CONFIDENCE")
                                .size(Theme::FONT_TINY)
                                .color(Theme::TEXT_MUTED)
                                .strong(),
                        );
                        ui.add_space(3.0);
                        let scene_color = if state.scene_label.contains("Unknown") {
                            Color32::from_rgb(255, 170, 0)
                        } else {
                            Theme::TEXT_ACCENT
                        };
                        ui.label(
                            RichText::new(format!(
                                "{} ({:.2})",
                                state.scene_label, state.confidence
                            ))
                            .size(Theme::FONT_BODY)
                            .color(scene_color)
                            .strong(),
                        );
                    });

                    // Col 2: Game & Focus (Topmost)
                    cols[1].vertical(|ui| {
                        ui.label(
                            RichText::new("GAME & FOCUS")
                                .size(Theme::FONT_TINY)
                                .color(Theme::TEXT_MUTED)
                                .strong(),
                        );
                        ui.add_space(3.0);
                        #[cfg(target_os = "windows")]
                        let (focus_txt, focus_color) = if state.is_active {
                            ("Active (Topmost)", Color32::from_rgb(100, 255, 150))
                        } else {
                            ("Inactive", Color32::from_rgb(255, 170, 0))
                        };
                        #[cfg(not(target_os = "windows"))]
                        let (focus_txt, focus_color) = if state.is_active {
                            ("Active", Color32::from_rgb(100, 255, 150))
                        } else {
                            ("Inactive", Color32::from_rgb(255, 170, 0))
                        };

                        let game_txt = if state.game_found {
                            "Found"
                        } else {
                            "Not Found"
                        };
                        ui.label(
                            RichText::new(format!("{} | {}", game_txt, focus_txt))
                                .size(Theme::FONT_BODY)
                                .color(focus_color)
                                .strong(),
                        );
                    });

                    // Col 3: Overlay Visibility & Capture Engine
                    cols[2].vertical(|ui| {
                        ui.label(
                            RichText::new("OVERLAY & ENGINE")
                                .size(Theme::FONT_TINY)
                                .color(Theme::TEXT_MUTED)
                                .strong(),
                        );
                        ui.add_space(3.0);
                        let (vis_txt, vis_color) = if state.overlay_on {
                            (
                                format!("Visible ({:.0}%)", state.opacity * 100.0),
                                Color32::from_rgb(100, 255, 150),
                            )
                        } else {
                            ("Hidden (0%)".to_string(), Color32::from_rgb(255, 100, 100))
                        };
                        #[cfg(target_os = "windows")]
                        let engine_str = state.capture_engine.to_uppercase();
                        #[cfg(not(target_os = "windows"))]
                        let engine_str = "XCOMPOSITE".to_string();

                        ui.label(
                            RichText::new(format!("{} | {}", vis_txt, engine_str))
                                .size(Theme::FONT_BODY)
                                .color(vis_color)
                                .strong(),
                        );
                    });
                });

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(10.0);

                ui.columns(3, |cols| {
                    // Row 2 - Col 1: Song & Jacket Status
                    cols[0].vertical(|ui| {
                        ui.label(
                            RichText::new("DETECTED SONG & MATCH")
                                .size(Theme::FONT_TINY)
                                .color(Theme::TEXT_MUTED)
                                .strong(),
                        );
                        ui.add_space(3.0);
                        let song_txt = if state.song_info.is_empty() {
                            "None".to_string()
                        } else {
                            state.song_info.clone()
                        };
                        let match_txt = if state.jacket_match_info.is_empty() {
                            "-".to_string()
                        } else {
                            state.jacket_match_info.clone()
                        };
                        ui.label(
                            RichText::new(format!("{} [{}]", song_txt, match_txt))
                                .size(Theme::FONT_BODY)
                                .color(Color32::from_rgb(255, 220, 100))
                                .strong(),
                        );
                    });

                    // Row 2 - Col 2: PlayState & Stability
                    cols[1].vertical(|ui| {
                        ui.label(
                            RichText::new("PLAYSTATE / STABILITY")
                                .size(Theme::FONT_TINY)
                                .color(Theme::TEXT_MUTED)
                                .strong(),
                        );
                        ui.add_space(3.0);
                        let ps_txt = if state.play_state_info.is_empty() {
                            "None".to_string()
                        } else {
                            state.play_state_info.clone()
                        };
                        ui.label(
                            RichText::new(ps_txt)
                                .size(Theme::FONT_BODY)
                                .color(Color32::from_rgb(180, 220, 255))
                                .strong(),
                        );
                    });

                    // Row 2 - Col 3: Capture Resolution & Geometry
                    cols[2].vertical(|ui| {
                        ui.label(
                            RichText::new("GEOMETRY & RESOLUTION")
                                .size(Theme::FONT_TINY)
                                .color(Theme::TEXT_MUTED)
                                .strong(),
                        );
                        ui.add_space(3.0);
                        let res_txt = if state.capture_res_info.is_empty() {
                            "Unknown".to_string()
                        } else {
                            state.capture_res_info.clone()
                        };
                        ui.label(
                            RichText::new(res_txt)
                                .size(Theme::FONT_BODY)
                                .color(Theme::TEXT_PRIMARY)
                                .strong(),
                        );
                    });
                });

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(10.0);

                ui.columns(3, |cols| {
                    // Row 3 - Col 1: Top 1 Jacket Similarity & Threshold
                    cols[0].vertical(|ui| {
                        ui.label(
                            RichText::new("TOP 1 JACKET SIMILARITY")
                                .size(Theme::FONT_TINY)
                                .color(Theme::TEXT_MUTED)
                                .strong(),
                        );
                        ui.add_space(3.0);
                        let (sim_txt, sim_color) = match state.top_jacket_similarity {
                            Some(sim) if sim >= 0.75 => (
                                format!("{:.3} (Pass >= 0.75)", sim),
                                Color32::from_rgb(100, 255, 150),
                            ),
                            Some(sim) => (
                                format!("{:.3} (Fail < 0.75)", sim),
                                Color32::from_rgb(255, 120, 120),
                            ),
                            None => ("None".to_string(), Theme::TEXT_MUTED),
                        };
                        ui.label(
                            RichText::new(sim_txt)
                                .size(Theme::FONT_BODY)
                                .color(sim_color)
                                .strong(),
                        );
                    });

                    // Row 3 - Col 2: Stable Hits Counter
                    cols[1].vertical(|ui| {
                        ui.label(
                            RichText::new("STABILITY HITS COUNTER")
                                .size(Theme::FONT_TINY)
                                .color(Theme::TEXT_MUTED)
                                .strong(),
                        );
                        ui.add_space(3.0);
                        let (hit_txt, hit_color) = if state.stable_hits >= 3 {
                            (
                                format!("{} / 3 (Stable)", state.stable_hits),
                                Color32::from_rgb(100, 255, 150),
                            )
                        } else {
                            (
                                format!("{} / 3 (Acquiring)", state.stable_hits),
                                Color32::from_rgb(255, 180, 50),
                            )
                        };
                        ui.label(
                            RichText::new(hit_txt)
                                .size(Theme::FONT_BODY)
                                .color(hit_color)
                                .strong(),
                        );
                    });

                    // Row 3 - Col 3: ROI Scale & Offset Y
                    cols[2].vertical(|ui| {
                        ui.label(
                            RichText::new("ROI SCALE & OFFSET Y")
                                .size(Theme::FONT_TINY)
                                .color(Theme::TEXT_MUTED)
                                .strong(),
                        );
                        ui.add_space(3.0);
                        ui.label(
                            RichText::new(format!(
                                "Scale: {:.3}x | Offset Y: {}px",
                                state.roi_scale, state.roi_offset_y
                            ))
                            .size(Theme::FONT_BODY)
                            .color(Theme::TEXT_ACCENT)
                            .strong(),
                        );
                    });
                });

                if let Some(ref snap) = state.telemetry_snapshot {
                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(10.0);

                    ui.columns(4, |cols| {
                        // Col 1: Capture & Detect Timing
                        cols[0].vertical(|ui| {
                            ui.label(
                                RichText::new("CAPTURE / DETECT AVG (MAX)")
                                    .size(Theme::FONT_TINY)
                                    .color(Theme::TEXT_MUTED)
                                    .strong(),
                            );
                            ui.add_space(3.0);
                            ui.label(
                                RichText::new(format!(
                                    "Cap: {} (max {}) | Det: {} (max {})",
                                    overmax_detect::format_duration_us(snap.capture_avg_us),
                                    overmax_detect::format_duration_us(snap.capture_max_us),
                                    overmax_detect::format_duration_us(snap.detect_avg_us),
                                    overmax_detect::format_duration_us(snap.detect_max_us),
                                ))
                                .size(Theme::FONT_BODY)
                                .color(Color32::from_rgb(100, 220, 255))
                                .strong(),
                            );
                        });

                        // Col 2: Pipeline Sub-Timings
                        cols[1].vertical(|ui| {
                            ui.label(
                                RichText::new("PIPELINE STAGE AVG")
                                    .size(Theme::FONT_TINY)
                                    .color(Theme::TEXT_MUTED)
                                    .strong(),
                            );
                            ui.add_space(3.0);
                            ui.label(
                                RichText::new(format!(
                                    "Scene: {} | Jkt: {} | Play: {}",
                                    overmax_detect::format_duration_us(snap.scene_avg_us),
                                    overmax_detect::format_duration_us(snap.jacket_avg_us),
                                    overmax_detect::format_duration_us(snap.play_state_avg_us),
                                ))
                                .size(Theme::FONT_BODY)
                                .color(Theme::TEXT_PRIMARY)
                                .strong(),
                            );
                        });

                        // Col 3: Active vs Unknown Frames
                        cols[2].vertical(|ui| {
                            ui.label(
                                RichText::new("5S FRAMES (ACTIVE / UNKNOWN)")
                                    .size(Theme::FONT_TINY)
                                    .color(Theme::TEXT_MUTED)
                                    .strong(),
                            );
                            ui.add_space(3.0);
                            ui.label(
                                RichText::new(format!(
                                    "Active: {} | Unknown: {}",
                                    snap.active_frames, snap.unknown_frames
                                ))
                                .size(Theme::FONT_BODY)
                                .color(Color32::from_rgb(255, 200, 100))
                                .strong(),
                            );
                        });

                        // Col 4: Jacket Match Hits
                        cols[3].vertical(|ui| {
                            ui.label(
                                RichText::new("MATCH JACKET HITS (5S)")
                                    .size(Theme::FONT_TINY)
                                    .color(Theme::TEXT_MUTED)
                                    .strong(),
                            );
                            ui.add_space(3.0);
                            ui.label(
                                RichText::new(format!("{} hits", snap.match_jacket_count))
                                    .size(Theme::FONT_BODY)
                                    .color(Color32::from_rgb(255, 120, 200))
                                    .strong(),
                            );
                        });
                    });
                }
            });
        });
}

fn render_controls(
    ui: &mut egui::Ui,
    lines: &Arc<Mutex<VecDeque<Arc<str>>>>,
    paused: &Arc<AtomicBool>,
    filters: &Arc<Mutex<std::collections::HashMap<String, bool>>>,
) {
    ui.horizontal(|ui| {
        let is_paused = paused.load(Ordering::Relaxed);
        let pause_label = if is_paused { "▶ Resume" } else { "⏸ Pause" };
        let pause_btn = egui::Button::new(RichText::new(pause_label).size(Theme::FONT_TINY))
            .fill(if is_paused {
                Theme::PRIMARY
            } else {
                Theme::SECTION_BG
            })
            .corner_radius(CornerRadius::same(6));

        if ui.add(pause_btn).clicked() {
            paused.store(!is_paused, Ordering::Relaxed);
        }

        let clear_btn = egui::Button::new(RichText::new("🗑 Clear").size(Theme::FONT_TINY))
            .fill(Theme::SECTION_BG)
            .corner_radius(CornerRadius::same(6));

        if ui.add(clear_btn).clicked() {
            if let Ok(mut g) = lines.lock() {
                g.clear();
            }
        }

        ui.add_space(12.0);
        ui.label(
            RichText::new("Filter:")
                .size(Theme::FONT_TINY)
                .color(Theme::TEXT_MUTED),
        );

        let filter_keys = [
            "All", "Main", "Engine", "Win32", "Overlay", "VArchive", "Error",
        ];
        let mut filter_map = overmax_core::lock_or_recover(filters);

        for key in filter_keys {
            let active = if key == "All" {
                filter_map.values().all(|&v| v)
            } else {
                *filter_map.get(key).unwrap_or(&true)
            };

            let filter_btn = egui::Button::new(RichText::new(key).size(Theme::FONT_TINY))
                .fill(if active {
                    Color32::from_rgb(0, 150, 180)
                } else {
                    Theme::SECTION_BG
                })
                .corner_radius(CornerRadius::same(6));

            if ui.add(filter_btn).clicked() {
                if key == "All" {
                    let next = !active;
                    for v in filter_map.values_mut() {
                        *v = next;
                    }
                } else {
                    let next = !active;
                    filter_map.insert(key.to_string(), next);
                }
            }
        }
    });
}

fn log_scroll(
    ui: &mut egui::Ui,
    lines: &Arc<Mutex<VecDeque<Arc<str>>>>,
    filters: &Arc<Mutex<std::collections::HashMap<String, bool>>>,
) {
    let raw_lines: Vec<Arc<str>> = if let Ok(g) = lines.lock() {
        g.iter().cloned().collect()
    } else {
        Vec::new()
    };

    let filter_map = overmax_core::lock_or_recover(filters);
    let filtered_lines: Vec<&Arc<str>> = raw_lines
        .iter()
        .filter(|line| {
            let l = line.as_ref();
            if l.contains("ERR") || l.contains("Error") || l.contains("fail") {
                *filter_map.get("Error").unwrap_or(&true)
            } else if l.contains("[Engine]") || l.contains("[Detection]") {
                *filter_map.get("Engine").unwrap_or(&true)
            } else if l.contains("[Win32]") || l.contains("[DPI]") {
                *filter_map.get("Win32").unwrap_or(&true)
            } else if l.contains("[Overlay]") {
                *filter_map.get("Overlay").unwrap_or(&true)
            } else if l.contains("[V-Archive]") || l.contains("[Sync]") {
                *filter_map.get("VArchive").unwrap_or(&true)
            } else {
                *filter_map.get("Main").unwrap_or(&true)
            }
        })
        .collect();

    Frame::new()
        .fill(Color32::from_rgb(12, 14, 20))
        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(30, 35, 48)))
        .corner_radius(CornerRadius::same(Theme::R_SM))
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            ScrollArea::vertical()
                .auto_shrink([false; 2])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    for line in filtered_lines {
                        let text = line.as_ref();
                        let color = if text.contains("ERR")
                            || text.contains("Error")
                            || text.contains("fail")
                        {
                            Color32::from_rgb(255, 100, 100)
                        } else if text.contains("[Win32]") || text.contains("[DPI]") {
                            Color32::from_rgb(180, 180, 255)
                        } else if text.contains("[Overlay]") {
                            Theme::TEXT_ACCENT
                        } else {
                            Theme::TEXT_PRIMARY
                        };

                        ui.label(
                            RichText::new(text)
                                .font(egui::FontId::monospace(11.0))
                                .color(color),
                        );
                    }
                });
        });
}
