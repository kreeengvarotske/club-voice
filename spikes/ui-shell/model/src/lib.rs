//! Toolkit-independent client view model for the Club Voice UI shell spike.
//!
//! The snapshot types follow the v5 client contract posted in Issue #1. The
//! server snapshot is authoritative for rooms, membership and roles; speaking
//! state, local mutes, per-member volume and collapsed rooms are client-local.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::time::Duration;

use serde::Deserialize;

pub type RoomId = u32;
pub type MemberId = u64;

/// Deepest room nesting the tree renders. Deeper rooms are clamped so a
/// malformed snapshot cannot produce unbounded indentation.
pub const MAX_DEPTH: usize = 8;

/// Bound on retained chat lines per room view.
pub const CHAT_HISTORY_LIMIT: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Member,
    RoomManager,
    Moderator,
    RoomManagerModerator,
    Owner,
}

impl Role {
    pub fn manages_rooms(self) -> bool {
        matches!(self, Role::RoomManager | Role::RoomManagerModerator | Role::Owner)
    }

    pub fn moderates(self) -> bool {
        matches!(self, Role::Moderator | Role::RoomManagerModerator | Role::Owner)
    }

    /// Short badge for the tree. `None` for ordinary members.
    pub fn badge(self) -> Option<&'static str> {
        match self {
            Role::Member => None,
            Role::RoomManager => Some("Manager"),
            Role::Moderator => Some("Mod"),
            Role::RoomManagerModerator => Some("Manager, Mod"),
            Role::Owner => Some("Owner"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct Access {
    pub manager_entry: bool,
    pub manager_voice: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Member {
    pub id: MemberId,
    pub name: String,
    pub role: Role,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Room {
    pub id: RoomId,
    pub name: String,
    pub parent: Option<RoomId>,
    pub access: Access,
    pub locked: bool,
    pub members: Vec<Member>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Snapshot {
    pub current_room: RoomId,
    pub member_id: MemberId,
    pub role: Role,
    pub voice_epoch: u64,
    pub rooms: Vec<Room>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub enum Reply {
    Ok(Snapshot),
    Err(String),
}

pub fn parse_reply(json: &str) -> Result<Reply, serde_json::Error> {
    serde_json::from_str(json)
}

/// Whether `viewer` may move or disconnect `target`, per the v5 contract:
/// moderators act on ordinary members, owners on any non-owner. The server
/// remains the authority; this only decides which actions the UI offers.
pub fn can_moderate(viewer: Role, target: Role) -> bool {
    match viewer {
        Role::Owner => target != Role::Owner,
        Role::Moderator | Role::RoomManagerModerator => {
            matches!(target, Role::Member | Role::RoomManager)
        }
        Role::Member | Role::RoomManager => false,
    }
}

pub fn can_enter(room: &Room, viewer: Role) -> bool {
    !room.access.manager_entry || viewer.manages_rooms()
}

pub fn can_talk(room: &Room, viewer: Role) -> bool {
    !room.access.manager_voice || viewer.manages_rooms()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowKind {
    Room {
        id: RoomId,
        locked: bool,
        entry_restricted: bool,
        voice_restricted: bool,
        member_count: usize,
        collapsed: bool,
        current: bool,
    },
    Member {
        id: MemberId,
        room: RoomId,
        role: Role,
        is_self: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub depth: usize,
    pub label: String,
    pub kind: RowKind,
}

/// Flattens the snapshot into tree rows: rooms in snapshot order under their
/// parent, occupants directly below their room. Rooms whose parent is missing
/// or that sit on a parent cycle are shown at the root so nobody disappears.
pub fn tree_rows(snapshot: &Snapshot, collapsed: &BTreeSet<RoomId>) -> Vec<Row> {
    let known: BTreeSet<RoomId> = snapshot.rooms.iter().map(|r| r.id).collect();
    let mut children: BTreeMap<Option<RoomId>, Vec<&Room>> = BTreeMap::new();
    for room in &snapshot.rooms {
        let parent = match room.parent {
            Some(p) if p != room.id && known.contains(&p) && !on_cycle(snapshot, room.id) => Some(p),
            _ => None,
        };
        children.entry(parent).or_default().push(room);
    }

    let mut rows = Vec::new();
    let mut visited = BTreeSet::new();
    let mut stack: Vec<(&Room, usize)> = children
        .get(&None)
        .map(|v| v.iter().rev().map(|r| (*r, 0)).collect())
        .unwrap_or_default();
    while let Some((room, depth)) = stack.pop() {
        if !visited.insert(room.id) {
            continue;
        }
        let is_collapsed = collapsed.contains(&room.id);
        rows.push(Row {
            depth,
            label: room.name.clone(),
            kind: RowKind::Room {
                id: room.id,
                locked: room.locked,
                entry_restricted: room.access.manager_entry,
                voice_restricted: room.access.manager_voice,
                member_count: room.members.len(),
                collapsed: is_collapsed,
                current: room.id == snapshot.current_room,
            },
        });
        if is_collapsed {
            continue;
        }
        let member_depth = (depth + 1).min(MAX_DEPTH);
        for m in &room.members {
            rows.push(Row {
                depth: member_depth,
                label: m.name.clone(),
                kind: RowKind::Member {
                    id: m.id,
                    room: room.id,
                    role: m.role,
                    is_self: m.id == snapshot.member_id,
                },
            });
        }
        if let Some(kids) = children.get(&Some(room.id)) {
            for kid in kids.iter().rev() {
                stack.push((*kid, (depth + 1).min(MAX_DEPTH)));
            }
        }
    }
    rows
}

fn on_cycle(snapshot: &Snapshot, start: RoomId) -> bool {
    let parent_of: HashMap<RoomId, Option<RoomId>> =
        snapshot.rooms.iter().map(|r| (r.id, r.parent)).collect();
    let mut seen = BTreeSet::new();
    let mut cur = Some(start);
    while let Some(id) = cur {
        if !seen.insert(id) {
            return id == start;
        }
        cur = parent_of.get(&id).copied().flatten();
    }
    false
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatLine {
    pub author: String,
    pub time: String,
    pub text: String,
}

/// Client-local state that the server snapshot does not carry.
#[derive(Debug, Clone)]
pub struct LocalState {
    pub collapsed: BTreeSet<RoomId>,
    pub speaking: BTreeSet<MemberId>,
    pub locally_muted: BTreeSet<MemberId>,
    /// Per-member playback gain in percent, 0..=200. Missing means 100.
    pub volume: BTreeMap<MemberId, u8>,
    pub selected: Option<MemberId>,
    pub mic_on: bool,
    pub sound_on: bool,
    pub ptt_down: bool,
    pub chat: Vec<ChatLine>,
}

impl Default for LocalState {
    fn default() -> Self {
        Self {
            collapsed: BTreeSet::new(),
            speaking: BTreeSet::new(),
            locally_muted: BTreeSet::new(),
            volume: BTreeMap::new(),
            selected: None,
            mic_on: true,
            sound_on: true,
            ptt_down: false,
            chat: Vec::new(),
        }
    }
}

impl LocalState {
    pub fn volume_of(&self, id: MemberId) -> u8 {
        self.volume.get(&id).copied().unwrap_or(100)
    }

    pub fn set_volume(&mut self, id: MemberId, percent: u8) {
        let p = percent.min(200);
        if p == 100 {
            self.volume.remove(&id);
        } else {
            self.volume.insert(id, p);
        }
    }

    pub fn toggle_collapsed(&mut self, room: RoomId) {
        if !self.collapsed.remove(&room) {
            self.collapsed.insert(room);
        }
    }

    pub fn push_chat(&mut self, line: ChatLine) {
        self.chat.push(line);
        let excess = self.chat.len().saturating_sub(CHAT_HISTORY_LIMIT);
        if excess > 0 {
            self.chat.drain(..excess);
        }
    }

    /// Applies a new authoritative snapshot. A changed current room resets
    /// room-local chat and participant controls, as the contract requires.
    pub fn on_snapshot(&mut self, previous: Option<&Snapshot>, next: &Snapshot) {
        let present: BTreeSet<MemberId> = next
            .rooms
            .iter()
            .flat_map(|r| r.members.iter().map(|m| m.id))
            .collect();
        self.speaking.retain(|id| present.contains(id));
        if previous.is_some_and(|p| p.current_room != next.current_room) {
            self.chat.clear();
            self.selected = None;
        }
        if self.selected.is_some_and(|id| !present.contains(&id)) {
            self.selected = None;
        }
    }
}

/// Deterministic speaking simulation for the "speaking" scenario: at each
/// tick a rotating pair of non-self members in the current room talks.
pub fn simulated_speakers(snapshot: &Snapshot, tick: u64) -> BTreeSet<MemberId> {
    let Some(room) = snapshot.rooms.iter().find(|r| r.id == snapshot.current_room) else {
        return BTreeSet::new();
    };
    let others: Vec<MemberId> = room
        .members
        .iter()
        .map(|m| m.id)
        .filter(|id| *id != snapshot.member_id)
        .collect();
    if others.is_empty() {
        return BTreeSet::new();
    }
    let n = others.len() as u64;
    [tick % n, (tick / 3) % n]
        .into_iter()
        .map(|i| others[i as usize])
        .collect()
}

/// Speaking indicator update period in the "speaking" scenario.
pub const SPEAKING_TICK: Duration = Duration::from_millis(250);

pub fn room_name(snapshot: &Snapshot, id: RoomId) -> &str {
    snapshot
        .rooms
        .iter()
        .find(|r| r.id == id)
        .map(|r| r.name.as_str())
        .unwrap_or("Unknown room")
}

pub fn member_name(snapshot: &Snapshot, id: MemberId) -> Option<(&str, Role)> {
    snapshot
        .rooms
        .iter()
        .flat_map(|r| r.members.iter())
        .find(|m| m.id == id)
        .map(|m| (m.name.as_str(), m.role))
}

/// The club demo snapshot used by every shell: the v5 example extended with
/// more rooms and occupants, including Cyrillic names to exercise fonts.
pub const DEMO_SNAPSHOT: &str = include_str!("../../fixtures/club-demo.json");

/// The exact example from the v5 contract.
pub const CONTRACT_EXAMPLE: &str = include_str!("../../fixtures/v5-contract-example.json");

pub fn demo_snapshot() -> Snapshot {
    match parse_reply(DEMO_SNAPSHOT) {
        Ok(Reply::Ok(s)) => s,
        other => panic!("bundled demo snapshot is invalid: {other:?}"),
    }
}

pub fn demo_chat() -> Vec<ChatLine> {
    let line = |a: &str, t: &str, x: &str| ChatLine {
        author: a.into(),
        time: t.into(),
        text: x.into(),
    };
    vec![
        line("Лена", "21:02", "Rotate to B after this round."),
        line("Костя", "21:03", "OK, I will take mid."),
        line("Alex", "21:03", "Push-to-talk is on Mouse 4, ping me if I cut out."),
        line("Тимур", "21:05", "Back in two minutes, sound off."),
    ]
}

/// Command-line options shared by all shells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scenario {
    /// Nothing changes after the first frame. Redraws should stop.
    Idle,
    /// Speaking indicators change every [`SPEAKING_TICK`].
    Speaking,
}

#[derive(Debug, Clone, Copy)]
pub struct Options {
    pub scenario: Scenario,
    pub exit_after: Option<Duration>,
}

impl Options {
    pub fn from_args() -> Self {
        let mut scenario = Scenario::Idle;
        let mut exit_after = None;
        let mut args = std::env::args().skip(1);
        while let Some(a) = args.next() {
            match a.as_str() {
                "--scenario" => match args.next().as_deref() {
                    Some("speaking") => scenario = Scenario::Speaking,
                    Some("idle") => scenario = Scenario::Idle,
                    other => panic!("unknown scenario {other:?}; use idle or speaking"),
                },
                "--exit-after" => {
                    let secs: u64 = args
                        .next()
                        .and_then(|s| s.parse().ok())
                        .expect("--exit-after takes whole seconds");
                    exit_after = Some(Duration::from_secs(secs));
                }
                other => panic!("unknown argument {other}"),
            }
        }
        Self {
            scenario,
            exit_after,
        }
    }
}

/// Counts frames and reports one JSON line on stdout at exit, so the
/// measurement script can compare redraw behavior across toolkits.
#[derive(Debug)]
pub struct FrameStats {
    toolkit: &'static str,
    scenario: Scenario,
    started: std::time::Instant,
    first_frame: Option<Duration>,
    frames: u64,
}

impl FrameStats {
    pub fn new(toolkit: &'static str, scenario: Scenario) -> Self {
        Self {
            toolkit,
            scenario,
            started: std::time::Instant::now(),
            first_frame: None,
            frames: 0,
        }
    }

    pub fn frame(&mut self) {
        self.frames += 1;
        if self.first_frame.is_none() {
            self.first_frame = Some(self.started.elapsed());
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    pub fn report(&self) -> String {
        format!(
            "{{\"toolkit\":\"{}\",\"scenario\":\"{}\",\"first_frame_ms\":{},\"frames\":{},\"run_ms\":{}}}",
            self.toolkit,
            match self.scenario {
                Scenario::Idle => "idle",
                Scenario::Speaking => "speaking",
            },
            self.first_frame.map(|d| d.as_millis() as i64).unwrap_or(-1),
            self.frames,
            self.started.elapsed().as_millis()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contract() -> Snapshot {
        match parse_reply(CONTRACT_EXAMPLE).unwrap() {
            Reply::Ok(s) => s,
            Reply::Err(e) => panic!("{e}"),
        }
    }

    #[test]
    fn parses_contract_example_and_error() {
        let s = contract();
        assert_eq!(s.current_room, 3);
        assert_eq!(s.member_id, 17);
        assert_eq!(s.rooms[3].members[1].role, Role::RoomManagerModerator);
        assert_eq!(
            parse_reply(r#"{"Err":"entry_restricted"}"#).unwrap(),
            Reply::Err("entry_restricted".into())
        );
    }

    #[test]
    fn tree_puts_subroom_and_occupants_under_parent() {
        let s = contract();
        let rows = tree_rows(&s, &BTreeSet::new());
        let labels: Vec<(usize, &str)> = rows.iter().map(|r| (r.depth, r.label.as_str())).collect();
        assert_eq!(
            labels,
            vec![
                (0, "Lobby"),
                (0, "Team A"),
                (1, "Briefing"),
                (2, "Alex"),
                (2, "Sam"),
                (0, "Team B"),
            ]
        );
        assert!(matches!(rows[2].kind, RowKind::Room { current: true, locked: true, voice_restricted: true, .. }));
        assert!(matches!(rows[3].kind, RowKind::Member { is_self: true, .. }));
    }

    #[test]
    fn collapsed_room_hides_members_and_subrooms() {
        let s = contract();
        let collapsed = BTreeSet::from([1]);
        let rows = tree_rows(&s, &collapsed);
        let labels: Vec<&str> = rows.iter().map(|r| r.label.as_str()).collect();
        assert_eq!(labels, vec!["Lobby", "Team A", "Team B"]);
    }

    #[test]
    fn missing_parent_and_cycles_stay_visible() {
        let mut s = contract();
        s.rooms[2].parent = Some(99);
        s.rooms[1].parent = Some(3); // Team A -> Briefing -> Team A
        let rows = tree_rows(&s, &BTreeSet::new());
        let rooms: BTreeSet<&str> = rows
            .iter()
            .filter(|r| matches!(r.kind, RowKind::Room { .. }))
            .map(|r| r.label.as_str())
            .collect();
        assert_eq!(rooms, BTreeSet::from(["Lobby", "Team A", "Team B", "Briefing"]));
    }

    #[test]
    fn moderation_matrix_matches_contract() {
        use Role::*;
        assert!(!can_moderate(Member, Member));
        assert!(!can_moderate(RoomManager, Member));
        assert!(can_moderate(Moderator, Member));
        assert!(!can_moderate(Moderator, Moderator));
        assert!(!can_moderate(Moderator, Owner));
        assert!(can_moderate(Owner, RoomManagerModerator));
        assert!(!can_moderate(Owner, Owner));
    }

    #[test]
    fn restricted_room_allows_listening_but_not_talking() {
        let s = contract();
        let briefing = &s.rooms[3];
        assert!(can_enter(briefing, Role::Member));
        assert!(!can_talk(briefing, Role::Member));
        assert!(can_talk(briefing, Role::RoomManager));
    }

    #[test]
    fn room_change_resets_room_local_state() {
        let before = contract();
        let mut after = before.clone();
        after.current_room = 1;
        let mut local = LocalState::default();
        local.push_chat(ChatLine { author: "a".into(), time: "t".into(), text: "x".into() });
        local.selected = Some(21);
        local.on_snapshot(Some(&before), &after);
        assert!(local.chat.is_empty());
        assert_eq!(local.selected, None);
    }

    #[test]
    fn chat_history_is_bounded_and_volume_clamped() {
        let mut local = LocalState::default();
        for i in 0..(CHAT_HISTORY_LIMIT + 5) {
            local.push_chat(ChatLine { author: "a".into(), time: "t".into(), text: i.to_string() });
        }
        assert_eq!(local.chat.len(), CHAT_HISTORY_LIMIT);
        assert_eq!(local.chat[0].text, "5");
        local.set_volume(7, 250);
        assert_eq!(local.volume_of(7), 200);
        local.set_volume(7, 100);
        assert!(local.volume.is_empty());
    }

    #[test]
    fn demo_snapshot_is_valid() {
        let s = demo_snapshot();
        assert!(s.rooms.iter().any(|r| r.id == s.current_room));
        assert!(!simulated_speakers(&s, 0).is_empty());
    }
}
