//! Developer control protocol over localhost UDP. Each datagram is one request.
use crate::{AdmissionError, RoomServer, ServerConfig};
use club_voice_protocol::JoinRequest;
use club_voice_protocol::media::SessionCommand;
use std::{
    collections::HashMap,
    io,
    net::{Ipv4Addr, SocketAddr, UdpSocket},
    time::{Duration, Instant},
};

pub struct LocalHost {
    socket: UdpSocket,
    rooms: RoomServer,
    peers: HashMap<SocketAddr, Peer>,
    last_stream: u64,
}

struct Peer {
    name: String,
    seen: Instant,
    stream: u64,
}

impl LocalHost {
    pub fn bind(port: u16, config: ServerConfig) -> io::Result<Self> {
        Ok(Self {
            socket: UdpSocket::bind((Ipv4Addr::LOCALHOST, port))?,
            rooms: RoomServer::new(config),
            peers: HashMap::new(),
            last_stream: 0,
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
        self.peers.retain(|_, peer| {
            let live = now.duration_since(peer.seen) < Duration::from_secs(30);
            if !live {
                self.rooms.leave_lobby(&peer.name);
            }
            live
        });
        let response = match &packet[..size] {
            bytes if bytes.starts_with(club_voice_protocol::media::MAGIC) => {
                if let Some((header, _)) = club_voice_protocol::media::Header::parse(bytes)
                    && let Some(sender) = self.peers.get_mut(&peer)
                    && sender.stream == header.stream
                    && sender.stream != 0
                {
                    sender.seen = now;
                    for (other, _) in self
                        .peers
                        .iter()
                        .filter(|(other, p)| **other != peer && p.stream != 0)
                    {
                        self.socket.send_to(bytes, other)?;
                    }
                }
                return Ok(());
            }
            bytes if let Some((command, id)) = SessionCommand::parse(bytes) => {
                match self.peers.get_mut(&peer) {
                    Some(p) if p.stream == id => match command {
                        SessionCommand::Ping => {
                            p.seen = now;
                            "PONG"
                        }
                        SessionCommand::Leave => {
                            let p = self.peers.remove(&peer).unwrap();
                            self.rooms.leave_lobby(&p.name);
                            "LEFT"
                        }
                    },
                    Some(_) => return Ok(()),
                    None => "JOIN_REQUIRED",
                }
            }
            b"CV1 LEAVE" if self.peers.get(&peer).is_none_or(|p| p.stream == 0) => {
                if let Some(p) = self.peers.remove(&peer) {
                    self.rooms.leave_lobby(&p.name);
                }
                "LEFT"
            }
            b"CV1 PING" => match self.peers.get_mut(&peer) {
                Some(p) if p.stream == 0 => {
                    p.seen = now;
                    "PONG"
                }
                _ => "JOIN_REQUIRED",
            },
            bytes if bytes.len() <= 192 && bytes.starts_with(b"CV2 JOIN\0") => {
                let status = self.join(peer, &bytes[9..], now, true);
                if status == "LOBBY" {
                    let mut ack = *b"CV2A00000000";
                    ack[4..].copy_from_slice(&self.peers[&peer].stream.to_be_bytes());
                    self.socket.send_to(&ack, peer)?;
                    return Ok(());
                }
                status
            }
            bytes if bytes.len() <= 192 && bytes.starts_with(b"CV1 JOIN\0") => {
                self.join(peer, &bytes[9..], now, false)
            }
            _ => "INVALID",
        };
        self.socket.send_to(response.as_bytes(), peer)?;
        Ok(())
    }

    fn join(&mut self, peer: SocketAddr, packet: &[u8], now: Instant, media: bool) -> &'static str {
        let Some((name, password)) = std::str::from_utf8(packet)
            .ok()
            .and_then(|s| s.split_once('\0'))
        else {
            return "INVALID";
        };
        if password != self.rooms.config.password {
            return "DENIED";
        }
        if let Some(existing) = self.peers.get_mut(&peer) {
            if existing.name != name || (existing.stream != 0) != media {
                return "LEAVE_FIRST";
            }
            existing.seen = now;
            return "LOBBY";
        }
        match self.rooms.admit(JoinRequest {
            display_name: name.into(),
            password: password.into(),
        }) {
            Ok(()) => {
                self.last_stream = self
                    .last_stream
                    .checked_add(1)
                    .expect("stream counter exhausted");
                self.peers.insert(
                    peer,
                    Peer {
                        name: name.into(),
                        seen: now,
                        stream: if media { self.last_stream } else { 0 },
                    },
                );
                "LOBBY"
            }
            Err(AdmissionError::ServerFull) => "FULL",
            Err(_) => "INVALID",
        }
    }
}
