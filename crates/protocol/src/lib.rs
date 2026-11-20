//! Bounded, allocation-conscious protocol primitives.
//!
//! Audio transport will be added separately. It must never reuse the control
//! queue below: stale media must be discarded at the playout boundary instead.

use std::collections::VecDeque;

pub const DEFAULT_ROOMS: [&str; 3] = ["Lobby", "Team A", "Team B"];
pub const MAX_CONTROL_BYTES: usize = 4 * 1024;

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
