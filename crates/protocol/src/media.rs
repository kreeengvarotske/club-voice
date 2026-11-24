//! Experimental single-speaker lobby datagram. Integers use network byte order.
//! The timestamp is a same-host measurement marker for the synthetic smoke run.
pub const MAGIC: &[u8; 4] = b"CVM2";
pub const HEADER: usize = 25;
pub const MAX_PAYLOAD: usize = 1275;
pub const MAX_DATAGRAM: usize = HEADER + MAX_PAYLOAD;

pub enum SessionCommand {
    Ping,
    Leave,
}

impl SessionCommand {
    pub fn packet(self, stream: u64) -> [u8; 12] {
        let mut bytes = [0; 12];
        bytes[..4].copy_from_slice(match self {
            Self::Ping => b"CV2P",
            Self::Leave => b"CV2L",
        });
        bytes[4..].copy_from_slice(&stream.to_be_bytes());
        bytes
    }
    pub fn parse(bytes: &[u8]) -> Option<(Self, u64)> {
        let (tag, id) = bytes.split_at_checked(4)?;
        let command = match tag {
            b"CV2P" => Self::Ping,
            b"CV2L" => Self::Leave,
            _ => return None,
        };
        let stream = u64::from_be_bytes(id.try_into().ok()?);
        (stream != 0).then_some((command, stream))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Header {
    pub stream: u64,
    pub frame_ms: u8,
    pub sequence: u32,
    pub sent_us: u64,
}

impl Header {
    pub fn write(self, out: &mut [u8; MAX_DATAGRAM]) {
        out[..4].copy_from_slice(MAGIC);
        out[4] = self.frame_ms;
        out[5..9].copy_from_slice(&self.sequence.to_be_bytes());
        out[9..17].copy_from_slice(&self.sent_us.to_be_bytes());
        out[17..HEADER].copy_from_slice(&self.stream.to_be_bytes());
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
                sent_us: u64::from_be_bytes(packet[9..17].try_into().ok()?),
                stream: u64::from_be_bytes(packet[17..HEADER].try_into().ok()?),
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
            stream: 11,
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
