//! Developer control protocol over localhost UDP. Each datagram is one request.
use crate::{AdmissionError, RoomServer, ServerConfig};
use club_voice_protocol::JoinRequest;
use std::{
    collections::HashMap,
    io,
    net::{Ipv4Addr, SocketAddr, UdpSocket},
    time::{Duration, Instant},
};

pub struct LocalHost {
    socket: UdpSocket,
    rooms: RoomServer,
    peers: HashMap<SocketAddr, (String, Instant)>,
}

impl LocalHost {
    pub fn bind(port: u16, config: ServerConfig) -> io::Result<Self> {
        Ok(Self {
            socket: UdpSocket::bind((Ipv4Addr::LOCALHOST, port))?,
            rooms: RoomServer::new(config),
            peers: HashMap::new(),
        })
    }

    pub fn address(&self) -> io::Result<SocketAddr> {
        self.socket.local_addr()
    }

    pub fn serve_one(&mut self) -> io::Result<()> {
        let mut packet = [0; 65535];
        self.serve_datagram(&mut packet)
    }

    pub fn serve(&mut self) -> io::Result<()> {
        // Windows rejects truncated datagrams. Reuse a full UDP buffer across frames.
        let mut packet = [0; 65535];
        loop {
            self.serve_datagram(&mut packet)?;
        }
    }

    fn serve_datagram(&mut self, packet: &mut [u8; 65535]) -> io::Result<()> {
        let (size, peer) = match self.socket.recv_from(packet) {
            // Windows reports ICMP port-unreachable from a previous send here.
            Err(e) if cfg!(windows) && e.kind() == io::ErrorKind::ConnectionReset => return Ok(()),
            other => other?,
        };
        let now = Instant::now();
        self.peers.retain(|_, (name, seen)| {
            let live = now.duration_since(*seen) < Duration::from_secs(30);
            if !live {
                self.rooms.leave_lobby(name);
            }
            live
        });
        let response = match &packet[..size] {
            bytes if bytes.starts_with(club_voice_protocol::media::MAGIC) => {
                if club_voice_protocol::media::Header::parse(bytes).is_some()
                    && let Some((_, seen)) = self.peers.get_mut(&peer)
                {
                    *seen = now;
                    for other in self.peers.keys().filter(|other| **other != peer) {
                        self.socket.send_to(bytes, other)?;
                    }
                }
                return Ok(());
            }
            b"CV1 LEAVE" => {
                if let Some((name, _)) = self.peers.remove(&peer) {
                    self.rooms.leave_lobby(&name);
                }
                "LEFT"
            }
            b"CV1 PING" => match self.peers.get_mut(&peer) {
                Some((_, seen)) => {
                    *seen = now;
                    "PONG"
                }
                None => "JOIN_REQUIRED",
            },
            bytes if bytes.len() <= 192 => self.join(peer, bytes, now),
            _ => "INVALID",
        };
        self.socket.send_to(response.as_bytes(), peer)?;
        Ok(())
    }

    fn join(&mut self, peer: SocketAddr, packet: &[u8], now: Instant) -> &'static str {
        let Some((name, password)) = std::str::from_utf8(packet)
            .ok()
            .and_then(|s| s.strip_prefix("CV1 JOIN\0"))
            .and_then(|s| s.split_once('\0'))
        else {
            return "INVALID";
        };
        if password != self.rooms.config.password {
            return "DENIED";
        }
        if let Some((existing, seen)) = self.peers.get_mut(&peer) {
            if existing != name {
                return "LEAVE_FIRST";
            }
            *seen = now;
            return "LOBBY";
        }
        match self.rooms.admit(JoinRequest {
            display_name: name.into(),
            password: password.into(),
        }) {
            Ok(()) => {
                self.peers.insert(peer, (name.into(), now));
                "LOBBY"
            }
            Err(AdmissionError::ServerFull) => "FULL",
            Err(_) => "INVALID",
        }
    }
}
