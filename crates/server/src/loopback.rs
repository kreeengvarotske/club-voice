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
        // A full UDP receive buffer also handles oversized datagrams on Windows.
        let mut packet = [0; 65535];
        let (size, peer) = self.socket.recv_from(&mut packet)?;
        let now = Instant::now();
        self.peers.retain(|_, (name, seen)| {
            let live = now.duration_since(*seen) < Duration::from_secs(30);
            if !live {
                self.rooms.leave_lobby(name);
            }
            live
        });
        let response = match &packet[..size] {
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
