//! Opt-in release benchmark. No real profiles, settings writes or audio taps.
use crate::{egui, MacEQ};
use std::time::Instant;

unsafe extern "C" {
    fn me_memory_snapshot(phase: u32) -> i32;
}

pub struct Probe {
    start: Instant,
    step: usize,
}
impl Default for Probe {
    fn default() -> Self {
        Self {
            start: Instant::now(),
            step: 0,
        }
    }
}
impl Probe {
    pub fn start_hidden(&mut self) {
        self.step = 1;
    }
}
impl MacEQ {
    pub fn memory_step(&mut self, ctx: &egui::Context) {
        let probe = &mut self.memory_probe;
        if probe.step >= 4 || probe.start.elapsed().as_secs() < [7, 17, 24, 34][probe.step] {
            return;
        }
        let step = probe.step;
        probe.step += 1;
        assert_eq!(
            unsafe { me_memory_snapshot(step as u32) },
            1,
            "Editor size was not restored/compacted"
        );
        if step == 3 {
            self.quitting = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        let visible = step == 1;
        self.editor_visible = visible;
        self.platform
            .as_mut()
            .unwrap()
            .set_editor_visible(visible)
            .unwrap();
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(visible));
    }
}
