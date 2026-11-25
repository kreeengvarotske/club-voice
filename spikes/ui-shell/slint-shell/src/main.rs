//! Club Voice main window shell on Slint (retained, declarative UI).

// No console window for testers; redirected stdout still receives the report.
#![windows_subsystem = "windows"]

use std::cell::RefCell;
use std::rc::Rc;

use cv_ui_model as model;
use model::{LocalState, Options, RowKind, Scenario, Snapshot};
use slint::{Model, ModelRc, SharedString, VecModel};

slint::slint! {
    #[edition = "1.19"]
    import { Button, CheckBox, LineEdit, ListView, Palette, ScrollView, Slider } from "std-widgets.slint";

    export struct TreeRow {
        depth: int,
        label: string,
        initial: string,
        is-room: bool,
        id: int,
        locked: bool,
        tag: string,
        count: string,
        collapsed: bool,
        current: bool,
        speaking: bool,
        is-self: bool,
        badge: string,
        note: string,
    }

    export struct ChatRow { author: string, time: string, text: string }

    component Pill inherits Rectangle {
        in property <string> text;
        in property <color> fg: #9AA3AE;
        border-radius: 4px;
        border-width: 1px;
        border-color: #3A424D;
        height: 18px;
        width: t.preferred-width + 8px;
        t := Text { text: root.text; color: root.fg; font-size: 11px; vertical-alignment: center; }
    }

    export component MainWindow inherits Window {
        title: "Club Voice (Slint shell)";
        preferred-width: 1280px;
        preferred-height: 760px;
        min-width: 720px;
        min-height: 480px;
        background: #0E1115;
        default-font-size: 14px;

        in property <[TreeRow]> rows;
        in property <[ChatRow]> chat;
        in property <string> room-title;
        in property <string> room-status;
        in property <bool> mic-on: true;
        in property <bool> sound-on: true;
        in property <bool> popover-visible;
        in property <length> popover-y;
        in property <string> popover-name;
        in property <float> popover-volume: 100;
        in property <bool> popover-muted;
        in property <bool> popover-can-moderate;

        callback toggle-room(int);
        callback select-member(int, length);
        callback close-popover();
        callback send(string);
        callback toggle-mic();
        callback toggle-sound();
        callback set-volume(float);
        callback set-local-mute(bool);

        init => { Palette.color-scheme = ColorScheme.dark; }

        VerticalLayout {
            Rectangle {
                height: 48px;
                background: #0B0D10;
                HorizontalLayout {
                    padding-left: 12px; padding-right: 12px; spacing: 12px;
                    alignment: start;
                    Text { text: "Club Voice"; color: #F5B041; font-weight: 700; vertical-alignment: center; }
                    Button { text: "Arena Club   LAN · 192.168.1.20:40772"; }
                    Button { text: "Saved servers"; }
                    Rectangle { horizontal-stretch: 1; }
                    Text { text: "● Connected"; color: #4FD1A5; vertical-alignment: center; }
                    Button { text: "Settings"; }
                }
            }
            HorizontalLayout {
                vertical-stretch: 1;
                Rectangle {
                    width: 296px;
                    background: #13171C;
                    VerticalLayout {
                        padding: 6px;
                        Text { text: "ARENA CLUB"; color: #9AA3AE; font-size: 12px; font-weight: 600; height: 28px; vertical-alignment: center; }
                        ListView {
                            for row in root.rows: Rectangle {
                                height: row.is-room ? 32px : 30px;
                                background: row.current ? #2A2418 : (touch.has-hover ? #1B2027 : transparent);
                                border-radius: 6px;
                                touch := TouchArea {
                                    clicked => {
                                        if (row.is-room) { root.toggle-room(row.id); }
                                        else if (!row.is-self) { root.select-member(row.id, self.absolute-position.y); }
                                    }
                                }
                                if row.current: Rectangle { x: 0; width: 3px; background: #F5B041; }
                                HorizontalLayout {
                                    padding-left: 8px + row.depth * 16px + (row.is-room ? 0px : 22px);
                                    padding-right: 8px;
                                    spacing: 8px;
                                    if row.is-room: Text { text: row.collapsed ? "▸" : "▾"; color: #7C8591; vertical-alignment: center; }
                                    if !row.is-room: Rectangle {
                                        width: 22px; height: 22px; y: 4px;
                                        border-radius: 11px;
                                        background: #3B4A63;
                                        border-width: row.speaking ? 2px : 0px;
                                        border-color: #4FD1A5;
                                        Text { text: row.initial; color: #E6E8EB; font-size: 11px; font-weight: 600; }
                                    }
                                    Text {
                                        text: row.label;
                                        color: row.speaking || row.current ? #FFFFFF : (row.is-room ? #C4CAD2 : #D3D7DC);
                                        font-weight: row.speaking || row.current ? 600 : 400;
                                        vertical-alignment: center;
                                        overflow: elide;
                                        horizontal-stretch: 1;
                                    }
                                    if row.is-self: Text { text: "you"; color: #9AA3AE; font-size: 11px; vertical-alignment: center; }
                                    if row.badge != "": VerticalLayout { alignment: center; Pill { text: row.badge; } }
                                    if row.locked: Text { text: "🔒"; color: #E8B86A; vertical-alignment: center; accessible-role: text; accessible-label: "Password required"; }
                                    if row.tag != "": VerticalLayout { alignment: center; Pill { text: row.tag; } }
                                    if row.note != "": Text { text: row.note; color: #E8B86A; font-size: 11px; vertical-alignment: center; }
                                    Text { text: row.count; color: #9AA3AE; font-size: 12px; vertical-alignment: center; }
                                }
                            }
                        }
                    }
                }
                Rectangle {
                    background: #171B21;
                    horizontal-stretch: 1;
                    VerticalLayout {
                        padding-left: 20px; padding-right: 20px; padding-top: 12px; padding-bottom: 16px;
                        spacing: 12px;
                        HorizontalLayout {
                            spacing: 12px;
                            alignment: start;
                            Text { text: root.room-title; font-size: 18px; font-weight: 600; color: #E6E8EB; }
                            Text { text: root.room-status; color: #8FE3C3; vertical-alignment: center; }
                        }
                        Rectangle { height: 1px; background: #222831; }
                        ScrollView {
                            vertical-stretch: 1;
                            VerticalLayout {
                                spacing: 14px;
                                alignment: start;
                                for line in root.chat: VerticalLayout {
                                    HorizontalLayout {
                                        spacing: 8px;
                                        alignment: start;
                                        Text { text: line.author; font-weight: 600; color: #E6E8EB; }
                                        Text { text: line.time; font-size: 12px; color: #7C8591; vertical-alignment: center; }
                                    }
                                    Text { text: line.text; color: #D3D7DC; wrap: word-wrap; }
                                }
                            }
                        }
                        input := LineEdit {
                            placeholder-text: "Message this room";
                            accepted(text) => { root.send(text); self.text = ""; }
                        }
                    }
                }
            }
            Rectangle {
                height: 64px;
                background: #0B0D10;
                HorizontalLayout {
                    padding-left: 12px; padding-right: 12px; spacing: 8px;
                    VerticalLayout {
                        width: 260px;
                        alignment: center;
                        Text { text: "Alex"; font-weight: 600; color: #E6E8EB; }
                        Text { text: "Voice connected"; font-size: 12px; color: #9AA3AE; }
                    }
                    Rectangle { horizontal-stretch: 1; }
                    VerticalLayout { alignment: center; Button { text: root.mic-on ? "Mic on" : "Mic off"; clicked => { root.toggle-mic(); } } }
                    VerticalLayout { alignment: center; Button { text: root.sound-on ? "Sound on" : "Sound off"; clicked => { root.toggle-sound(); } } }
                    Text { text: "PTT · Mouse 4"; color: #E8B86A; vertical-alignment: center; }
                    VerticalLayout { alignment: center; Button { text: "Devices"; } }
                    Rectangle { horizontal-stretch: 1; }
                    VerticalLayout {
                        alignment: center;
                        Rectangle {
                            width: 120px; height: 40px; border-radius: 10px; background: #C62F35;
                            Text { text: "Leave room"; color: #FFFFFF; font-weight: 600; }
                            TouchArea {}
                        }
                    }
                }
            }
        }

        if root.popover-visible: Rectangle {
            x: 290px;
            y: root.popover-y;
            width: 260px;
            height: pop.preferred-height;
            background: #1F242C;
            border-radius: 10px;
            border-width: 1px;
            border-color: #343C47;
            drop-shadow-blur: 24px;
            drop-shadow-color: #00000080;
            pop := VerticalLayout {
                padding: 14px; spacing: 10px;
                HorizontalLayout {
                    Text { text: root.popover-name; font-weight: 600; color: #E6E8EB; horizontal-stretch: 1; }
                    Button { text: "Close"; clicked => { root.close-popover(); } }
                }
                Text { text: "Volume for you: " + round(root.popover-volume) + "%"; color: #C4CAD2; font-size: 13px; }
                Slider {
                    minimum: 0; maximum: 200; value: root.popover-volume;
                    changed(v) => { root.set-volume(v); }
                }
                CheckBox { text: "Mute for me"; checked: root.popover-muted; toggled => { root.set-local-mute(self.checked); } }
                if root.popover-can-moderate: Button { text: "Move to room…"; }
                if root.popover-can-moderate: Button { text: "Disconnect from server"; }
            }
        }
    }
}

struct Shell {
    snapshot: Snapshot,
    local: LocalState,
    rows: Rc<VecModel<TreeRow>>,
    chat: Rc<VecModel<ChatRow>>,
}

impl Shell {
    fn rebuild_rows(&self) {
        let rows: Vec<TreeRow> = model::tree_rows(&self.snapshot, &self.local.collapsed)
            .into_iter()
            .map(|row| match row.kind {
                RowKind::Room { id, locked, entry_restricted, voice_restricted, member_count, collapsed, current } => TreeRow {
                    depth: row.depth as i32,
                    label: row.label.into(),
                    is_room: true,
                    id: id as i32,
                    locked,
                    tag: if entry_restricted { "Managers".into() } else if voice_restricted { "Listen only".into() } else { SharedString::new() },
                    count: member_count.to_string().into(),
                    collapsed,
                    current,
                    ..Default::default()
                },
                RowKind::Member { id, role, is_self, .. } => {
                    let vol = self.local.volume_of(id);
                    let mut note = String::new();
                    if vol != 100 {
                        note = format!("{vol}%");
                    }
                    if self.local.locally_muted.contains(&id) {
                        note = "muted for you".into();
                    }
                    TreeRow {
                        depth: row.depth as i32,
                        initial: row.label.chars().take(1).collect::<String>().into(),
                        label: row.label.into(),
                        is_room: false,
                        id: id as i32,
                        speaking: self.local.speaking.contains(&id),
                        is_self,
                        badge: role.badge().unwrap_or("").into(),
                        note: note.into(),
                        ..Default::default()
                    }
                }
            })
            .collect();
        // Update in place so unchanged rows do not re-instantiate.
        if self.rows.row_count() == rows.len() {
            for (i, r) in rows.into_iter().enumerate() {
                if self.rows.row_data(i).as_ref() != Some(&r) {
                    self.rows.set_row_data(i, r);
                }
            }
        } else {
            self.rows.set_vec(rows);
        }
    }

    fn rebuild_chat(&self) {
        self.chat.set_vec(
            self.local
                .chat
                .iter()
                .map(|l| ChatRow { author: l.author.as_str().into(), time: l.time.as_str().into(), text: l.text.as_str().into() })
                .collect::<Vec<_>>(),
        );
    }
}

fn main() -> Result<(), slint::PlatformError> {
    let options = Options::from_args();
    let stats = Rc::new(RefCell::new(model::FrameStats::new("slint", options.scenario)));
    let snapshot = model::demo_snapshot();
    let mut local = LocalState::default();
    for line in model::demo_chat() {
        local.push_chat(line);
    }
    let ui = MainWindow::new()?;
    let shell = Rc::new(RefCell::new(Shell {
        snapshot,
        local,
        rows: Rc::new(VecModel::default()),
        chat: Rc::new(VecModel::default()),
    }));
    {
        let s = shell.borrow();
        s.rebuild_rows();
        s.rebuild_chat();
        ui.set_rows(ModelRc::from(s.rows.clone()));
        ui.set_chat(ModelRc::from(s.chat.clone()));
        let room = s.snapshot.rooms.iter().find(|r| r.id == s.snapshot.current_room);
        ui.set_room_title(model::room_name(&s.snapshot, s.snapshot.current_room).into());
        let status = match room {
            Some(r) if model::can_talk(r, s.snapshot.role) => format!("{} people · You can talk", r.members.len()),
            Some(r) => format!("{} people · Listen only", r.members.len()),
            None => String::new(),
        };
        ui.set_room_status(status.into());
    }

    let weak = ui.as_weak();
    let sh = shell.clone();
    ui.on_toggle_room(move |id| {
        sh.borrow_mut().local.toggle_collapsed(id as u32);
        sh.borrow().rebuild_rows();
        let _ = &weak;
    });

    let weak = ui.as_weak();
    let sh = shell.clone();
    ui.on_select_member(move |id, y| {
        let ui = weak.unwrap();
        let mut s = sh.borrow_mut();
        let id = id as u64;
        if s.local.selected == Some(id) {
            s.local.selected = None;
            ui.set_popover_visible(false);
            return;
        }
        s.local.selected = Some(id);
        if let Some((name, role)) = model::member_name(&s.snapshot, id) {
            ui.set_popover_name(name.into());
            ui.set_popover_can_moderate(model::can_moderate(s.snapshot.role, role));
        }
        ui.set_popover_volume(s.local.volume_of(id) as f32);
        ui.set_popover_muted(s.local.locally_muted.contains(&id));
        ui.set_popover_y(y);
        ui.set_popover_visible(true);
    });

    let weak = ui.as_weak();
    let sh = shell.clone();
    ui.on_close_popover(move || {
        sh.borrow_mut().local.selected = None;
        weak.unwrap().set_popover_visible(false);
    });

    let weak = ui.as_weak();
    let sh = shell.clone();
    ui.on_set_volume(move |v| {
        let mut s = sh.borrow_mut();
        if let Some(id) = s.local.selected {
            s.local.set_volume(id, v.round().clamp(0.0, 200.0) as u8);
            drop(s);
            sh.borrow().rebuild_rows();
            weak.unwrap().set_popover_volume(v.round());
        }
    });

    let sh = shell.clone();
    ui.on_set_local_mute(move |muted| {
        let mut s = sh.borrow_mut();
        if let Some(id) = s.local.selected {
            if muted {
                s.local.locally_muted.insert(id);
            } else {
                s.local.locally_muted.remove(&id);
            }
            drop(s);
            sh.borrow().rebuild_rows();
        }
    });

    let sh = shell.clone();
    ui.on_send(move |text| {
        if text.trim().is_empty() {
            return;
        }
        sh.borrow_mut().local.push_chat(model::ChatLine { author: "Alex".into(), time: "now".into(), text: text.to_string() });
        sh.borrow().rebuild_chat();
    });

    let weak = ui.as_weak();
    let sh = shell.clone();
    ui.on_toggle_mic(move || {
        let mut s = sh.borrow_mut();
        s.local.mic_on = !s.local.mic_on;
        weak.unwrap().set_mic_on(s.local.mic_on);
    });
    let weak = ui.as_weak();
    let sh = shell.clone();
    ui.on_toggle_sound(move || {
        let mut s = sh.borrow_mut();
        s.local.sound_on = !s.local.sound_on;
        weak.unwrap().set_sound_on(s.local.sound_on);
    });

    let st = stats.clone();
    ui.window()
        .set_rendering_notifier(move |state, _| {
            if matches!(state, slint::RenderingState::AfterRendering) {
                st.borrow_mut().frame();
            }
        })
        .map_err(|e| slint::PlatformError::Other(format!("rendering notifier: {e:?}")))?;

    let speaking_timer = slint::Timer::default();
    if options.scenario == Scenario::Speaking {
        let sh = shell.clone();
        let tick = std::cell::Cell::new(0u64);
        speaking_timer.start(slint::TimerMode::Repeated, model::SPEAKING_TICK, move || {
            let t = tick.get();
            tick.set(t + 1);
            let mut s = sh.borrow_mut();
            let speakers = model::simulated_speakers(&s.snapshot, t);
            s.local.speaking = speakers;
            drop(s);
            sh.borrow().rebuild_rows();
        });
    }
    if let Some(limit) = options.exit_after {
        slint::Timer::single_shot(limit, || {
            let _ = slint::quit_event_loop();
        });
    }

    ui.run()?;
    println!("{}", stats.borrow().report());
    Ok(())
}
