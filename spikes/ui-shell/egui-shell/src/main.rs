//! Club Voice main window shell on egui/eframe (immediate mode, glow backend).

// No console window for testers; redirected stdout still receives the report.
#![windows_subsystem = "windows"]

use std::collections::BTreeSet;

use cv_ui_model as model;
use eframe::egui::{self, Color32, CornerRadius, RichText, Stroke, Vec2};
use model::{LocalState, Options, RowKind, Scenario, Snapshot};

const BG: Color32 = Color32::from_rgb(0x0E, 0x11, 0x15);
const BAR: Color32 = Color32::from_rgb(0x0B, 0x0D, 0x10);
const SIDEBAR: Color32 = Color32::from_rgb(0x13, 0x17, 0x1C);
const CENTER: Color32 = Color32::from_rgb(0x17, 0x1B, 0x21);
const RAISED: Color32 = Color32::from_rgb(0x20, 0x25, 0x2D);
const LINE: Color32 = Color32::from_rgb(0x22, 0x28, 0x31);
const TEXT: Color32 = Color32::from_rgb(0xE6, 0xE8, 0xEB);
const MUTED: Color32 = Color32::from_rgb(0x9A, 0xA3, 0xAE);
const ACCENT: Color32 = Color32::from_rgb(0xF5, 0xB0, 0x41);
const ACTIVE_ROW: Color32 = Color32::from_rgb(0x2A, 0x24, 0x18);
const SPEAKING: Color32 = Color32::from_rgb(0x4F, 0xD1, 0xA5);
const OFF: Color32 = Color32::from_rgb(0xFF, 0x8A, 0x80);
const DANGER: Color32 = Color32::from_rgb(0xC6, 0x2F, 0x35);

struct ShellApp {
    options: Options,
    snapshot: Snapshot,
    local: LocalState,
    stats: model::FrameStats,
    draft: String,
    popover_anchor: Option<egui::Pos2>,
    last_tick: u64,
}

impl ShellApp {
    fn new(options: Options) -> Self {
        let snapshot = model::demo_snapshot();
        let mut local = LocalState::default();
        for line in model::demo_chat() {
            local.push_chat(line);
        }
        Self {
            stats: model::FrameStats::new("egui", options.scenario),
            options,
            snapshot,
            local,
            draft: String::new(),
            popover_anchor: None,
            last_tick: u64::MAX,
        }
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            ui.label(RichText::new("Club Voice").strong().color(ACCENT));
            ui.add_space(8.0);
            let _ = ui.button(
                RichText::new("Arena Club   LAN · 192.168.1.20:40772").color(TEXT),
            );
            let _ = ui.button("Saved servers");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let _ = ui.button("Settings");
                ui.label(RichText::new("Connected").color(SPEAKING));
                let (r, _) = ui.allocate_exact_size(Vec2::splat(10.0), egui::Sense::hover());
                ui.painter().circle_filled(r.center(), 4.0, SPEAKING);
            });
        });
    }

    fn tree(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("ARENA CLUB").small().strong().color(MUTED));
        ui.add_space(4.0);
        let rows = model::tree_rows(&self.snapshot, &self.local.collapsed);
        let viewer = self.snapshot.role;
        let mut toggle = None;
        let mut select = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for row in &rows {
                ui.horizontal(|ui| {
                    ui.add_space(row.depth as f32 * 16.0);
                    match &row.kind {
                        RowKind::Room { id, locked, entry_restricted, voice_restricted, member_count, collapsed, current } => {
                            // Default egui fonts lack arrow glyphs; paint the chevron.
                            let (rect, resp) = ui.allocate_exact_size(Vec2::new(14.0, 26.0), egui::Sense::click());
                            let c = rect.center();
                            let pts = if *collapsed {
                                vec![c + Vec2::new(-3.0, -5.0), c + Vec2::new(4.0, 0.0), c + Vec2::new(-3.0, 5.0)]
                            } else {
                                vec![c + Vec2::new(-5.0, -3.0), c + Vec2::new(5.0, -3.0), c + Vec2::new(0.0, 4.0)]
                            };
                            ui.painter().add(egui::Shape::convex_polygon(pts, MUTED, Stroke::NONE));
                            if resp.on_hover_text(if *collapsed { "Expand" } else { "Collapse" }).clicked() {
                                toggle = Some(*id);
                            }
                            let mut text = RichText::new(&row.label).color(if *current { TEXT } else { MUTED });
                            if *current {
                                text = text.strong();
                            }
                            let fill = if *current { ACTIVE_ROW } else { Color32::TRANSPARENT };
                            if ui.add(egui::Button::new(text).fill(fill).frame(*current)).clicked() {
                                toggle = Some(*id);
                            }
                            if *locked {
                                ui.label(RichText::new("Password").small().color(ACCENT)).on_hover_text("Password required");
                            }
                            if *entry_restricted {
                                ui.label(RichText::new("Managers").small().color(MUTED));
                            } else if *voice_restricted {
                                ui.label(RichText::new("Listen only").small().color(MUTED));
                            }
                            ui.label(RichText::new(member_count.to_string()).small().color(MUTED));
                        }
                        RowKind::Member { id, role, is_self, .. } => {
                            let speaking = self.local.speaking.contains(id);
                            let (rect, _) = ui.allocate_exact_size(Vec2::splat(20.0), egui::Sense::hover());
                            ui.painter().circle_filled(rect.center(), 9.0, RAISED);
                            if speaking {
                                ui.painter().circle_stroke(rect.center(), 10.0, Stroke::new(2.0, SPEAKING));
                            }
                            let initial: String = row.label.chars().take(1).collect();
                            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, initial, egui::FontId::proportional(11.0), TEXT);
                            let mut name = RichText::new(&row.label).color(if speaking { Color32::WHITE } else { TEXT });
                            if speaking {
                                name = name.strong();
                            }
                            let selected = self.local.selected == Some(*id);
                            let resp = ui.add(egui::Button::new(name).frame(selected).fill(RAISED));
                            if resp.clicked() && !*is_self {
                                select = Some((*id, resp.rect.right_top()));
                            }
                            if *is_self {
                                ui.label(RichText::new("you").small().color(MUTED));
                            }
                            if let Some(badge) = role.badge() {
                                ui.label(RichText::new(badge).small().color(MUTED));
                            }
                            let vol = self.local.volume_of(*id);
                            if vol != 100 {
                                ui.label(RichText::new(format!("{vol}%")).small().color(ACCENT));
                            }
                            if self.local.locally_muted.contains(id) {
                                ui.label(RichText::new("muted for you").small().color(OFF));
                            }
                            let _ = viewer;
                        }
                    }
                });
            }
        });
        if let Some(id) = toggle {
            self.local.toggle_collapsed(id);
        }
        if let Some((id, pos)) = select {
            if self.local.selected == Some(id) {
                self.local.selected = None;
                self.popover_anchor = None;
            } else {
                self.local.selected = Some(id);
                self.popover_anchor = Some(pos);
            }
        }
    }

    fn center(&mut self, ui: &mut egui::Ui) {
        let room = self
            .snapshot
            .rooms
            .iter()
            .find(|r| r.id == self.snapshot.current_room)
            .cloned();
        ui.horizontal(|ui| {
            ui.heading(model::room_name(&self.snapshot, self.snapshot.current_room));
            if let Some(room) = &room {
                ui.label(RichText::new(format!("{} people", room.members.len())).color(MUTED));
                if model::can_talk(room, self.snapshot.role) {
                    ui.label(RichText::new("You can talk").color(SPEAKING));
                } else {
                    ui.label(RichText::new("Listen only").color(MUTED));
                }
            }
        });
        ui.separator();
        let input_h = 44.0;
        let log_h = (ui.available_height() - input_h).max(0.0);
        egui::ScrollArea::vertical()
            .max_height(log_h)
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for line in &self.local.chat {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&line.author).strong().color(TEXT));
                        ui.label(RichText::new(&line.time).small().color(MUTED));
                    });
                    ui.label(RichText::new(&line.text).color(TEXT));
                    ui.add_space(8.0);
                }
            });
        let resp = ui.add_sized(
            [ui.available_width(), 36.0],
            egui::TextEdit::singleline(&mut self.draft).hint_text("Message this room"),
        );
        if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !self.draft.trim().is_empty() {
            let text = std::mem::take(&mut self.draft);
            self.local.push_chat(model::ChatLine { author: "Alex".into(), time: "now".into(), text });
            resp.request_focus();
        }
    }

    fn call_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            ui.label(RichText::new("Alex").strong());
            ui.label(RichText::new("Voice connected").small().color(MUTED));
            ui.add_space(24.0);
            let mic = if self.local.mic_on { "Mic on" } else { "Mic off" };
            if ui.add(egui::Button::new(mic).min_size(Vec2::new(80.0, 36.0))).clicked() {
                self.local.mic_on = !self.local.mic_on;
            }
            let sound = if self.local.sound_on { "Sound on" } else { "Sound off" };
            if ui.add(egui::Button::new(sound).min_size(Vec2::new(80.0, 36.0))).clicked() {
                self.local.sound_on = !self.local.sound_on;
            }
            let ptt = if self.local.ptt_down { "PTT · talking" } else { "PTT · Mouse 4" };
            ui.label(RichText::new(ptt).color(ACCENT));
            let _ = ui.add(egui::Button::new("Devices").min_size(Vec2::new(80.0, 36.0)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let _ = ui.add(
                    egui::Button::new(RichText::new("Leave room").color(Color32::WHITE).strong())
                        .fill(DANGER)
                        .min_size(Vec2::new(110.0, 36.0)),
                );
            });
        });
    }

    fn popover(&mut self, ctx: &egui::Context) {
        let (Some(id), Some(anchor)) = (self.local.selected, self.popover_anchor) else {
            return;
        };
        let Some((name, role)) = model::member_name(&self.snapshot, id).map(|(n, r)| (n.to_owned(), r)) else {
            return;
        };
        let viewer = self.snapshot.role;
        egui::Area::new(egui::Id::unique("member-popover"))
            .fixed_pos(anchor + Vec2::new(8.0, 0.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).fill(RAISED).show(ui, |ui| {
                    ui.set_width(240.0);
                    ui.label(RichText::new(&name).strong());
                    let mut vol = self.local.volume_of(id) as u32;
                    ui.label(RichText::new("Volume for you").color(MUTED));
                    if ui.add(egui::Slider::new(&mut vol, 0..=200).suffix("%")).changed() {
                        self.local.set_volume(id, vol as u8);
                    }
                    let mut muted = self.local.locally_muted.contains(&id);
                    if ui.checkbox(&mut muted, "Mute for me").changed() {
                        if muted {
                            self.local.locally_muted.insert(id);
                        } else {
                            self.local.locally_muted.remove(&id);
                        }
                    }
                    if model::can_moderate(viewer, role) {
                        ui.separator();
                        let _ = ui.button("Move to room…");
                        let _ = ui.button(RichText::new("Disconnect from server").color(OFF));
                    }
                });
            });
    }
}

impl eframe::App for ShellApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let elapsed = self.stats.elapsed();
        if let Some(limit) = self.options.exit_after {
            if elapsed >= limit {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
            ctx.request_repaint_after(limit - elapsed);
        }
        if self.options.scenario == Scenario::Speaking {
            let tick = (elapsed.as_millis() / model::SPEAKING_TICK.as_millis()) as u64;
            if tick != self.last_tick {
                self.last_tick = tick;
                self.local.speaking = model::simulated_speakers(&self.snapshot, tick);
            }
            let next = model::SPEAKING_TICK * (tick as u32 + 1);
            ctx.request_repaint_after(next.saturating_sub(elapsed));
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.local.selected = None;
            self.popover_anchor = None;
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.stats.frame();
        let frame = |fill| egui::Frame::new().fill(fill).inner_margin(egui::Margin::symmetric(12, 8));
        egui::Panel::top("top").exact_size(48.0).frame(frame(BAR)).show(ui, |ui| self.top_bar(ui));
        egui::Panel::bottom("call").exact_size(64.0).frame(frame(BAR)).show(ui, |ui| self.call_bar(ui));
        egui::Panel::left("tree")
            .exact_size(296.0)
            .frame(frame(SIDEBAR))
            .show(ui, |ui| self.tree(ui));
        egui::CentralPanel::default()
            .frame(frame(CENTER).inner_margin(egui::Margin::symmetric(20, 12)))
            .show(ui, |ui| self.center(ui));
        let ctx = ui.ctx().clone();
        self.popover(&ctx);
    }
}

impl Drop for ShellApp {
    fn drop(&mut self) {
        println!("{}", self.stats.report());
    }
}

fn style(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = CENTER;
    visuals.window_fill = RAISED;
    visuals.extreme_bg_color = RAISED;
    visuals.override_text_color = Some(TEXT);
    visuals.widgets.inactive.corner_radius = CornerRadius::same(6);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(6);
    visuals.widgets.active.corner_radius = CornerRadius::same(6);
    visuals.selection.bg_fill = Color32::from_rgb(0x5A, 0x48, 0x22);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
    ctx.set_visuals(visuals);
    let _ = BG;
}

fn main() -> eframe::Result {
    let options = Options::from_args();
    let native = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Club Voice (egui shell)")
            .with_inner_size([1280.0, 760.0])
            .with_min_inner_size([720.0, 480.0]),
        ..Default::default()
    };
    let _ = BTreeSet::<u32>::new();
    eframe::run_native(
        "Club Voice (egui shell)",
        native,
        Box::new(move |cc| {
            style(&cc.egui_ctx);
            Ok(Box::new(ShellApp::new(options)))
        }),
    )
}
