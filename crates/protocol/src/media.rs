//! Experimental single-speaker lobby datagram. Integers use network byte order.
//! The timestamp is a same-host measurement marker for the synthetic smoke run.
pub const MAGIC: &[u8; 4] = b"CVM1";
pub const HEADER: usize = 17;
pub const MAX_PAYLOAD: usize = 1275;
pub const MAX_DATAGRAM: usize = HEADER + MAX_PAYLOAD;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Header {
    pub frame_ms: u8,
    pub sequence: u32,
    pub sent_us: u64,
}

impl Header {
    pub fn write(self, out: &mut [u8; MAX_DATAGRAM]) {
        out[..4].copy_from_slice(MAGIC);
        out[4] = self.frame_ms;
        out[5..9].copy_from_slice(&self.sequence.to_be_bytes());
        out[9..HEADER].copy_from_slice(&self.sent_us.to_be_bytes());
    }

    pub fn parse(packet: &[u8]) -> Option<(Self, &[u8])> {
        if !(HEADER + 1..=MAX_DATAGRAM).contains(&packet.len())
            || !packet.starts_with(MAGIC)
            || !matches!(packet[4], 10 | 20)
        {
            return None;
        }
        Some((
            Self {
                frame_ms: packet[4],
                sequence: u32::from_be_bytes(packet[5..9].try_into().ok()?),
                sent_us: u64::from_be_bytes(packet[9..HEADER].try_into().ok()?),
            },
            &packet[HEADER..],
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounds_and_duration_are_checked_before_relay() {
        let mut data = [0; MAX_DATAGRAM];
        let header = Header {
            frame_ms: 10,
            sequence: 1234,
            sent_us: 5678,
        };
        header.write(&mut data);
        assert_eq!(Header::parse(&data).unwrap().0, header);
        assert!(Header::parse(&data[..HEADER]).is_none());
        assert!(Header::parse(&[0; MAX_DATAGRAM + 1]).is_none());
        data[4] = 0;
        assert!(Header::parse(&data).is_none());
    }
}
