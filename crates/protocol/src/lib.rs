//! Bounded, allocation-conscious protocol primitives.
//!
//! Audio transport will be added separately. It must never reuse the control
//! queue below: stale media must be discarded at the playout boundary instead.

use std::collections::VecDeque;
use std::net::{Ipv4Addr, SocketAddrV4};

pub mod media;

pub const DEFAULT_ROOMS: [&str; 3] = ["Lobby", "Team A", "Team B"];
pub const MAX_CONTROL_BYTES: usize = 4 * 1024;
pub const LAN_PORT: u16 = 40771;

pub fn parse_lan_address(input: &str) -> Result<SocketAddrV4, &'static str> {
    let input = input.trim();
    let address = if input.contains(':') {
        input.parse::<SocketAddrV4>()
    } else {
        input
            .parse::<Ipv4Addr>()
            .map(|ip| SocketAddrV4::new(ip, LAN_PORT))
    }
    .map_err(|_| "enter an IPv4 address or IPv4:port")?;
    if !address.ip().is_private() {
        return Err("enter a private IPv4 address for the LAN pilot");
    }
    Ok(address)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JoinRequest {
    pub display_name: String,
    pub password: String,
}

impl JoinRequest {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.display_name.trim().is_empty() {
            return Err(ProtocolError::EmptyDisplayName);
        }
        if self.display_name.len() > 48 {
            return Err(ProtocolError::DisplayNameTooLong);
        }
        if self.password.len() > 128 {
            return Err(ProtocolError::PasswordTooLong);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolError {
    EmptyDisplayName,
    DisplayNameTooLong,
    PasswordTooLong,
    ZeroCapacity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundedQueue<T> {
    capacity: usize,
    values: VecDeque<T>,
    dropped: u64,
}

impl<T> BoundedQueue<T> {
    pub fn new(capacity: usize) -> Result<Self, ProtocolError> {
        if capacity == 0 {
            return Err(ProtocolError::ZeroCapacity);
        }
        Ok(Self {
            capacity,
            values: VecDeque::with_capacity(capacity),
            dropped: 0,
        })
    }

    /// Keeps the newest control state while making loss observable.
    pub fn push_latest(&mut self, value: T) {
        if self.values.len() == self.capacity {
            self.values.pop_front();
            self.dropped += 1;
        }
        self.values.push_back(value);
    }

    pub fn pop(&mut self) -> Option<T> {
        self.values.pop_front()
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub fn dropped(&self) -> u64 {
        self.dropped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lan_address_accepts_bare_ipv4_and_preserves_explicit_ports() {
        assert_eq!(
            parse_lan_address(" 192.168.1.42 ").unwrap().port(),
            LAN_PORT
        );
        assert_eq!(parse_lan_address("10.0.0.2:40802").unwrap().port(), 40802);
        assert!(parse_lan_address("10.0.0.2:wrong").is_err());
        assert!(parse_lan_address("0.0.0.0:40771").is_err());
        assert!(parse_lan_address("127.0.0.1:40771").is_err());
    }

    #[test]
    fn rejects_an_empty_display_name() {
        let request = JoinRequest {
            display_name: "  ".into(),
            password: "club".into(),
        };

        assert_eq!(request.validate(), Err(ProtocolError::EmptyDisplayName));
    }

    #[test]
    fn retains_newest_control_messages_at_capacity() {
        let mut queue = BoundedQueue::new(2).unwrap();
        queue.push_latest("first");
        queue.push_latest("second");
        queue.push_latest("third");

        assert_eq!(queue.dropped(), 1);
        assert_eq!(queue.pop(), Some("second"));
        assert_eq!(queue.pop(), Some("third"));
    }
}
