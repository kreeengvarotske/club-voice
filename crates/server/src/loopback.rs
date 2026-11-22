//! Developer UDP relay for loopback and an explicitly selected private IPv4 interface.
use crate::{AdmissionError, RoomServer, ServerConfig};
use club_voice_protocol::JoinRequest;
use club_voice_protocol::media::SessionCommand;
use std::{
    collections::HashMap,
    io,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket},
    time::{Duration, Instant},
};

const PEER_IDLE_LIMIT: Duration = Duration::from_secs(30);

pub struct LocalHost {
    socket: UdpSocket,
    rooms: RoomServer,
    peers: HashMap<SocketAddr, Peer>,
    last_stream: u64,
    next_expiry: Option<Instant>,
}

struct Peer {
    name: String,
    seen: Instant,
    stream: u64,
}

impl LocalHost {
    pub fn bind(port: u16, config: ServerConfig) -> io::Result<Self> {
        Self::bind_address(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port), config)
    }

    pub fn bind_lan(address: SocketAddrV4, config: ServerConfig) -> io::Result<Self> {
        if !address.ip().is_private()
            || !(8..=128).contains(&config.password.len())
            || config.password.contains('\0')
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "LAN pilot needs a private IPv4 address and an 8–128 byte room password",
            ));
        }
        Self::bind_address(address, config)
    }

    fn bind_address(address: SocketAddrV4, config: ServerConfig) -> io::Result<Self> {
        Ok(Self {
            socket: UdpSocket::bind(address)?,
            rooms: RoomServer::new(config),
            peers: HashMap::new(),
            last_stream: 0,
            next_expiry: None,
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
        if !matches!(peer.ip(), std::net::IpAddr::V4(ip) if ip.is_loopback() || ip.is_private()) {
            return Ok(());
        }
        let now = Instant::now();
        self.expire_peers(now);
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

    fn expire_peers(&mut self, now: Instant) {
        if self.next_expiry.is_none_or(|deadline| now < deadline) {
            return;
        }
        self.next_expiry = None;
        self.peers.retain(|_, peer| {
            let deadline = peer.seen + PEER_IDLE_LIMIT;
            if now >= deadline {
                self.rooms.leave_lobby(&peer.name);
                return false;
            }
            self.next_expiry = Some(self.next_expiry.map_or(deadline, |next| next.min(deadline)));
            true
        });
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
                // Activity moves deadlines forward. An earlier cached deadline can stay
                // until the next scan, when we recalculate it for all surviving peers.
                self.next_expiry.get_or_insert(now + PEER_IDLE_LIMIT);
                "LOBBY"
            }
            Err(AdmissionError::ServerFull) => "FULL",
            Err(_) => "INVALID",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lan_listener_rejects_wildcard_addresses_and_short_passwords() {
        for (ip, password) in [
            (Ipv4Addr::UNSPECIFIED, "test-password"),
            (Ipv4Addr::new(192, 168, 1, 2), "short"),
        ] {
            let result = LocalHost::bind_lan(
                SocketAddrV4::new(ip, 0),
                ServerConfig {
                    name: "Test".into(),
                    password: password.into(),
                    member_limit: 2,
                },
            );
            assert!(matches!(result, Err(e) if e.kind() == io::ErrorKind::InvalidInput));
        }
    }

    #[test]
    fn expiry_releases_capacity_and_preserves_refreshed_sessions() {
        let mut host = LocalHost::bind(
            0,
            ServerConfig {
                name: "Test".into(),
                password: "test".into(),
                member_limit: 2,
            },
        )
        .unwrap();
        let alice = (Ipv4Addr::LOCALHOST, 10001).into();
        let bob = (Ipv4Addr::LOCALHOST, 10002).into();
        let start = Instant::now();
        let refreshed = start + Duration::from_secs(1);
        assert_eq!(host.join(alice, b"Alice\0test", start, true), "LOBBY");
        assert_eq!(host.join(bob, b"Bob\0test", start, true), "LOBBY");
        assert_eq!(host.join(bob, b"Bob\0test", refreshed, true), "LOBBY");
        let alice_stream = host.peers[&alice].stream;
        let deadline = start + PEER_IDLE_LIMIT;
        host.expire_peers(deadline - Duration::from_millis(1));
        assert_eq!(host.rooms.member_count(), 2);
        host.expire_peers(deadline);
        assert_eq!(host.rooms.member_count(), 1);
        assert!(!host.peers.contains_key(&alice));
        assert_eq!(host.next_expiry, Some(refreshed + PEER_IDLE_LIMIT));
        assert_eq!(host.join(alice, b"Alice\0test", deadline, true), "LOBBY");
        assert!(host.peers[&alice].stream > alice_stream);
        host.expire_peers(refreshed + PEER_IDLE_LIMIT);
        assert!(!host.peers.contains_key(&bob));
        assert!(host.peers.contains_key(&alice));
    }
}
