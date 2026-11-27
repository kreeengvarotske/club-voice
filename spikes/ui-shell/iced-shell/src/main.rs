//! Club Voice main window shell on iced (Elm-style, retained widget tree).

// No console window for testers; redirected stdout still receives the report.
#![windows_subsystem = "windows"]

use std::cell::Cell;

use cv_ui_model as model;
use iced::widget::{button, checkbox, column, container, mouse_area, row, scrollable, slider, space, stack, text, text_input};
use iced::{Border, Color, Element, Font, Length, Padding, Subscription, Task, Theme, font};
use model::{LocalState, MemberId, Options, RoomId, RowKind, Scenario, Snapshot};

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgb8(r, g, b)
}
const BAR: Color = rgb(0x0B, 0x0D, 0x10);
const SIDEBAR: Color = rgb(0x13, 0x17, 0x1C);
const CENTER: Color = rgb(0x17, 0x1B, 0x21);
const RAISED: Color = rgb(0x1F, 0x24, 0x2C);
const TEXT: Color = rgb(0xE6, 0xE8, 0xEB);
const MUTED: Color = rgb(0x9A, 0xA3, 0xAE);
const ACCENT: Color = rgb(0xF5, 0xB0, 0x41);
const ACTIVE_ROW: Color = rgb(0x2A, 0x24, 0x18);
const SPEAKING: Color = rgb(0x4F, 0xD1, 0xA5);
const OFF: Color = rgb(0xFF, 0x8A, 0x80);
const DANGER: Color = rgb(0xC6, 0x2F, 0x35);

const BOLD: Font = Font { weight: font::Weight::Semibold, ..Font::DEFAULT };

#[derive(Debug, Clone)]
enum Message {
    ToggleRoom(RoomId),
    Select(MemberId, f32),
    ClosePopover,
    Volume(u8),
    LocalMute(bool),
    Draft(String),
    Send,
    ToggleMic,
    ToggleSound,
    SpeakingTick,
    Exit,
    Noop,
}

struct Shell {
    options: Options,
    snapshot: Snapshot,
    local: LocalState,
    draft: String,
    popover_y: f32,
    tick: u64,
    stats: std::cell::RefCell<model::FrameStats>,
    views: Cell<u64>,
}

impl Shell {
    fn boot(options: Options) -> Self {
        let mut local = LocalState::default();
        for line in model::demo_chat() {
            local.push_chat(line);
        }
        Self {
            stats: std::cell::RefCell::new(model::FrameStats::new("iced", options.scenario)),
            options,
            snapshot: model::demo_snapshot(),
            local,
            draft: String::new(),
            popover_y: 0.0,
            tick: 0,
            views: Cell::new(0),
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::ToggleRoom(id) => self.local.toggle_collapsed(id),
            Message::Select(id, y) => {
                if self.local.selected == Some(id) {
                    self.local.selected = None;
                } else {
                    self.local.selected = Some(id);
                    self.popover_y = y;
                }
            }
            Message::ClosePopover => self.local.selected = None,
            Message::Volume(v) => {
                if let Some(id) = self.local.selected {
                    self.local.set_volume(id, v);
                }
            }
            Message::LocalMute(m) => {
                if let Some(id) = self.local.selected {
                    if m {
                        self.local.locally_muted.insert(id);
                    } else {
                        self.local.locally_muted.remove(&id);
                    }
                }
            }
            Message::Draft(d) => self.draft = d,
            Message::Send => {
                if !self.draft.trim().is_empty() {
                    let text = std::mem::take(&mut self.draft);
                    self.local.push_chat(model::ChatLine { author: "Alex".into(), time: "now".into(), text });
                }
            }
            Message::ToggleMic => self.local.mic_on = !self.local.mic_on,
            Message::ToggleSound => self.local.sound_on = !self.local.sound_on,
            Message::SpeakingTick => {
                self.local.speaking = model::simulated_speakers(&self.snapshot, self.tick);
                self.tick += 1;
            }
            Message::Exit => {
                println!("{}", self.stats.borrow().report());
                return iced::exit();
            }
            Message::Noop => {}
        }
        Task::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        let mut subs = Vec::new();
        if self.options.scenario == Scenario::Speaking {
            subs.push(iced::time::every(model::SPEAKING_TICK).map(|_| Message::SpeakingTick));
        }
        if let Some(limit) = self.options.exit_after {
            subs.push(iced::time::every(limit).map(|_| Message::Exit));
        }
        Subscription::batch(subs)
    }

    fn view(&self) -> Element<'_, Message> {
        self.views.set(self.views.get() + 1);
        self.stats.borrow_mut().frame();

        let top = container(
            row![
                text("Club Voice").font(BOLD).color(ACCENT),
                button(text("Arena Club   LAN · 192.168.1.20:40772")).on_press(Message::Noop).style(button::secondary),
                button(text("Saved servers")).on_press(Message::Noop).style(button::secondary),
                space::horizontal(),
                text("● Connected").color(SPEAKING),
                button(text("Settings")).on_press(Message::Noop).style(button::secondary),
            ]
            .spacing(12)
            .align_y(iced::Alignment::Center),
        )
        .padding(Padding::from([0, 12]))
        .height(48)
        .center_y(48)
        .style(fill(BAR));

        let mut tree = column![text("ARENA CLUB").size(12).font(BOLD).color(MUTED)].spacing(2);
        let mut y = 48.0 + 8.0 + 20.0;
        for r in model::tree_rows(&self.snapshot, &self.local.collapsed) {
            let indent = r.depth as f32 * 16.0;
            match r.kind {
                RowKind::Room { id, locked, entry_restricted, voice_restricted, member_count, collapsed, current } => {
                    let mut line = row![
                        text(if collapsed { "▸" } else { "▾" }).color(MUTED),
                        text(r.label.clone()).color(if current { Color::WHITE } else { TEXT }).font(if current { BOLD } else { Font::DEFAULT }),
                        space::horizontal(),
                    ]
                    .spacing(8)
                    .align_y(iced::Alignment::Center);
                    if locked {
                        line = line.push(text("🔒").color(ACCENT));
                    }
                    if entry_restricted {
                        line = line.push(text("Managers").size(11).color(MUTED));
                    } else if voice_restricted {
                        line = line.push(text("Listen only").size(11).color(MUTED));
                    }
                    line = line.push(text(member_count.to_string()).size(12).color(MUTED));
                    let b = button(line)
                        .width(Length::Fill)
                        .padding(Padding { left: 8.0 + indent, right: 8.0, top: 6.0, bottom: 6.0 })
                        .on_press(Message::ToggleRoom(id))
                        .style(move |_, status| button::Style {
                            background: Some(
                                if current { ACTIVE_ROW } else if matches!(status, button::Status::Hovered) { RAISED } else { Color::TRANSPARENT }
                                .into(),
                            ),
                            text_color: TEXT,
                            border: Border { radius: 6.0.into(), ..Border::default() },
                            ..button::Style::default()
                        });
                    tree = tree.push(b);
                    y += 34.0;
                }
                RowKind::Member { id, role, is_self, .. } => {
                    let speaking = self.local.speaking.contains(&id);
                    let initial: String = r.label.chars().take(1).collect();
                    let avatar = container(text(initial).size(11).font(BOLD).color(TEXT))
                        .width(22)
                        .height(22)
                        .center(22)
                        .style(move |_| container::Style {
                            background: Some(rgb(0x3B, 0x4A, 0x63).into()),
                            border: Border { radius: 11.0.into(), width: if speaking { 2.0 } else { 0.0 }, color: SPEAKING },
                            ..container::Style::default()
                        });
                    let mut line = row![
                        avatar,
                        text(r.label.clone()).color(if speaking { Color::WHITE } else { TEXT }).font(if speaking { BOLD } else { Font::DEFAULT }),
                    ]
                    .spacing(8)
                    .align_y(iced::Alignment::Center);
                    if is_self {
                        line = line.push(text("you").size(11).color(MUTED));
                    }
                    line = line.push(space::horizontal());
                    if let Some(badge) = role.badge() {
                        line = line.push(text(badge).size(11).color(MUTED));
                    }
                    let vol = self.local.volume_of(id);
                    if vol != 100 {
                        line = line.push(text(format!("{vol}%")).size(11).color(ACCENT));
                    }
                    if self.local.locally_muted.contains(&id) {
                        line = line.push(text("muted for you").size(11).color(OFF));
                    }
                    let selected = self.local.selected == Some(id);
                    let row_y = y;
                    let b = button(line)
                        .width(Length::Fill)
                        .padding(Padding { left: 30.0 + indent, right: 8.0, top: 3.0, bottom: 3.0 })
                        .on_press_maybe((!is_self).then_some(Message::Select(id, row_y)))
                        .style(move |_, status| button::Style {
                            background: Some(
                                if selected || matches!(status, button::Status::Hovered) { RAISED } else { Color::TRANSPARENT }.into(),
                            ),
                            text_color: TEXT,
                            border: Border { radius: 6.0.into(), ..Border::default() },
                            ..button::Style::default()
                        });
                    tree = tree.push(b);
                    y += 30.0;
                }
            }
        }
        let sidebar = container(scrollable(tree).height(Length::Fill))
            .width(296)
            .height(Length::Fill)
            .padding(6)
            .style(fill(SIDEBAR));

        let room = self.snapshot.rooms.iter().find(|r| r.id == self.snapshot.current_room);
        let status = match room {
            Some(r) if model::can_talk(r, self.snapshot.role) => format!("{} people · You can talk", r.members.len()),
            Some(r) => format!("{} people · Listen only", r.members.len()),
            None => String::new(),
        };
        let chat = column(self.local.chat.iter().map(|l| {
            column![
                row![text(l.author.as_str()).font(BOLD).color(TEXT), text(l.time.as_str()).size(12).color(MUTED)].spacing(8),
                text(l.text.as_str()).color(TEXT),
            ]
            .spacing(2)
            .into()
        }))
        .spacing(14);
        let center = container(
            column![
                row![
                    text(model::room_name(&self.snapshot, self.snapshot.current_room)).size(18).font(BOLD).color(TEXT),
                    text(status).color(SPEAKING),
                ]
                .spacing(12)
                .align_y(iced::Alignment::Center),
                scrollable(chat).height(Length::Fill).anchor_bottom(),
                text_input("Message this room", &self.draft).on_input(Message::Draft).on_submit(Message::Send).padding(10),
            ]
            .spacing(12),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(Padding { top: 12.0, bottom: 16.0, left: 20.0, right: 20.0 })
        .style(fill(CENTER));

        let call = container(
            row![
                column![text("Alex").font(BOLD).color(TEXT), text("Voice connected").size(12).color(MUTED)].width(260),
                space::horizontal(),
                button(text(if self.local.mic_on { "Mic on" } else { "Mic off" })).on_press(Message::ToggleMic).style(button::secondary).padding(10),
                button(text(if self.local.sound_on { "Sound on" } else { "Sound off" })).on_press(Message::ToggleSound).style(button::secondary).padding(10),
                text("PTT · Mouse 4").color(ACCENT),
                button(text("Devices")).on_press(Message::Noop).style(button::secondary).padding(10),
                space::horizontal(),
                button(text("Leave room").font(BOLD).color(Color::WHITE))
                    .on_press(Message::Noop)
                    .padding(Padding::from([10, 16]))
                    .style(|_, _| button::Style {
                        background: Some(DANGER.into()),
                        text_color: Color::WHITE,
                        border: Border { radius: 10.0.into(), ..Border::default() },
                        ..button::Style::default()
                    }),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center),
        )
        .padding(Padding::from([0, 12]))
        .height(64)
        .center_y(64)
        .style(fill(BAR));

        let base: Element<'_, Message> = column![top, row![sidebar, center].height(Length::Fill), call].into();

        let Some(id) = self.local.selected else {
            return base;
        };
        let Some((name, role)) = model::member_name(&self.snapshot, id) else {
            return base;
        };
        let mut pop = column![
            row![text(name).font(BOLD).color(TEXT), space::horizontal(), button(text("Close")).on_press(Message::ClosePopover).style(button::secondary)]
                .align_y(iced::Alignment::Center),
            text(format!("Volume for you: {}%", self.local.volume_of(id))).size(13).color(MUTED),
            slider(0..=200u8, self.local.volume_of(id), Message::Volume),
            checkbox(self.local.locally_muted.contains(&id)).label("Mute for me").on_toggle(Message::LocalMute),
        ]
        .spacing(10);
        if model::can_moderate(self.snapshot.role, role) {
            pop = pop.push(button(text("Move to room…")).on_press(Message::Noop).style(button::secondary));
            pop = pop.push(button(text("Disconnect from server").color(OFF)).on_press(Message::Noop).style(button::secondary));
        }
        let card = container(pop).width(260).padding(14).style(|_| container::Style {
            background: Some(RAISED.into()),
            border: Border { radius: 10.0.into(), width: 1.0, color: rgb(0x34, 0x3C, 0x47) },
            ..container::Style::default()
        });
        let overlay = container(mouse_area(card).on_press(Message::Noop))
            .padding(Padding { left: 290.0, top: self.popover_y, ..Padding::ZERO });
        stack![base, overlay].into()
    }
}

fn fill(color: Color) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style { background: Some(color.into()), ..container::Style::default() }
}

fn main() -> iced::Result {
    let options = Options::from_args();
    iced::application(move || Shell::boot(options), Shell::update, Shell::view)
        .title("Club Voice (iced shell)")
        .subscription(Shell::subscription)
        .theme(|_: &Shell| Theme::Dark)
        .window_size((1280.0, 760.0))
        .run()
}
