use crate::debug_ui;
use crate::native_app::NativeApp;
use eframe::egui;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};

impl NativeApp {
    pub(crate) fn start_log_pump(&mut self, ctx: &egui::Context) {
        let Some(rx) = self.log_rx.take() else {
            return;
        };
        let lines = self.log_lines.clone();
        let paused = self.debug_paused.clone();
        let ctx = ctx.clone();
        let max = self.max_log_lines();
        std::thread::spawn(move || {
            while let Ok(msg) = rx.recv() {
                if !paused.load(Ordering::Relaxed) {
                    debug_ui::push_log(&lines, max, msg);
                    drain_pending_logs(&lines, &rx, max, &paused);
                    ctx.request_repaint();
                }
            }
        });
    }
}

fn drain_pending_logs(
    lines: &Arc<Mutex<VecDeque<String>>>,
    rx: &Receiver<String>,
    max_lines: usize,
    paused: &Arc<AtomicBool>,
) {
    while let Ok(msg) = rx.try_recv() {
        if !paused.load(Ordering::Relaxed) {
            debug_ui::push_log(lines, max_lines, msg);
        }
    }
}
