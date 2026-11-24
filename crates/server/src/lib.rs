//! Local-room state and a loopback-only control prototype.
pub mod loopback;

use club_voice_protocol::{DEFAULT_ROOMS, JoinRequest, ProtocolError};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerConfig {
    pub name: String,
    pub password: String,
    pub member_limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoomSnapshot {
    pub name: String,
    pub members: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdmissionError {
    InvalidRequest(ProtocolError),
    WrongPassword,
    ServerFull,
}

pub struct RoomServer {
    config: ServerConfig,
    rooms: BTreeMap<String, Vec<String>>,
}

impl RoomServer {
    pub fn new(config: ServerConfig) -> Self {
        let rooms = DEFAULT_ROOMS
            .into_iter()
            .map(|name| (name.to_owned(), Vec::new()))
            .collect();
        Self { config, rooms }
    }

    pub fn admit(&mut self, request: JoinRequest) -> Result<(), AdmissionError> {
        request.validate().map_err(AdmissionError::InvalidRequest)?;
        if request.password != self.config.password {
            return Err(AdmissionError::WrongPassword);
        }
        if self.member_count() >= self.config.member_limit {
            return Err(AdmissionError::ServerFull);
        }
        self.rooms
            .get_mut("Lobby")
            .expect("default room exists")
            .push(request.display_name);
        Ok(())
    }

    pub fn rooms(&self) -> Vec<RoomSnapshot> {
        self.rooms
            .iter()
            .map(|(name, members)| RoomSnapshot {
                name: name.clone(),
                members: members.clone(),
            })
            .collect()
    }

    fn leave_lobby(&mut self, name: &str) {
        let lobby = self.rooms.get_mut("Lobby").expect("default room exists");
        if let Some(index) = lobby.iter().position(|member| member == name) {
            lobby.swap_remove(index);
        }
    }

    fn member_count(&self) -> usize {
        self.rooms.values().map(Vec::len).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(limit: usize) -> RoomServer {
        RoomServer::new(ServerConfig {
            name: "Club LAN".into(),
            password: "club".into(),
            member_limit: limit,
        })
    }

    #[test]
    fn creates_the_standard_team_rooms() {
        assert_eq!(
            server(2)
                .rooms()
                .into_iter()
                .map(|room| room.name)
                .collect::<Vec<_>>(),
            ["Lobby", "Team A", "Team B"]
        );
    }

    #[test]
    fn rejects_admission_after_the_member_limit() {
        let mut server = server(1);
        server
            .admit(JoinRequest {
                display_name: "Ada".into(),
                password: "club".into(),
            })
            .unwrap();

        assert_eq!(
            server.admit(JoinRequest {
                display_name: "Lin".into(),
                password: "club".into()
            }),
            Err(AdmissionError::ServerFull)
        );
    }
}
