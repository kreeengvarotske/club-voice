//! Client-side state that is safe to construct without opening an audio device.

use club_voice_protocol::BoundedQueue;

/// Developer protocol for the local, two-seat media probes.
/// Call before starting workers that use the connected socket.
pub fn join_loopback_voice(socket: &std::net::UdpSocket, name: &str) -> std::io::Result<u64> {
    if !socket.peer_addr()?.ip().is_loopback() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "expected a loopback server",
        ));
    }
    join_voice(socket, name, "test")
}

/// Join an explicitly addressed private IPv4 pilot server before starting media workers.
pub fn join_lan_voice(
    socket: &std::net::UdpSocket,
    name: &str,
    password: &str,
) -> std::io::Result<u64> {
    if !matches!(socket.peer_addr()?.ip(), std::net::IpAddr::V4(ip) if ip.is_private()) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "expected a private IPv4 server",
        ));
    }
    join_voice(socket, name, password)
}

fn join_voice(socket: &std::net::UdpSocket, name: &str, password: &str) -> std::io::Result<u64> {
    use club_voice_protocol::media::{Header, MAX_DATAGRAM};
    use std::io::{Error, ErrorKind};
    use std::time::{Duration, Instant};
    if name.trim().is_empty()
        || name.len() > 48
        || name.contains('\0')
        || password.len() > 128
        || password.contains('\0')
    {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "invalid display name or room password",
        ));
    }
    let original_timeout = socket.read_timeout()?;
    let deadline = Instant::now() + original_timeout.unwrap_or(Duration::from_secs(2));
    let request = format!("CV2 JOIN\0{name}\0{password}");
    let mut buf = [0; MAX_DATAGRAM + 1];
    let result = (|| {
        for attempts_left in (1..=3).rev() {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            let attempt_deadline = now + deadline.duration_since(now) / attempts_left;
            socket.send(request.as_bytes())?;
            loop {
                let wait = attempt_deadline.saturating_duration_since(Instant::now());
                if wait.is_zero() {
                    break;
                }
                socket.set_read_timeout(Some(wait.max(Duration::from_millis(1))))?;
                let n = match socket.recv(&mut buf) {
                    Ok(n) => n,
                    Err(e) if matches!(e.kind(), ErrorKind::TimedOut | ErrorKind::WouldBlock) => {
                        break;
                    }
                    Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                    Err(e) => return Err(e),
                };
                // The relay can forward audio before a retransmitted acknowledgement.
                if Header::parse(&buf[..n]).is_some() {
                    continue;
                }
                if n != 12 || &buf[..4] != b"CV2A" {
                    return Err(Error::new(
                        ErrorKind::InvalidData,
                        format!("join failed: {}", String::from_utf8_lossy(&buf[..n])),
                    ));
                }
                let stream = u64::from_be_bytes(buf[4..12].try_into().unwrap());
                return if stream == 0 {
                    Err(Error::new(ErrorKind::InvalidData, "zero voice session ID"))
                } else {
                    Ok(stream)
                };
            }
        }
        Err(Error::new(
            ErrorKind::TimedOut,
            "voice server did not acknowledge admission",
        ))
    })();
    let restored = socket.set_read_timeout(original_timeout);
    result.and_then(|stream| restored.map(|()| stream))
}

#[cfg(feature = "audio-device-probe")]
pub mod devices {
    //! Device enumeration only. This module never builds an input or output
    //! stream, so it cannot start microphone capture or playback.

    use cpal::traits::{DeviceTrait, HostTrait};

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct AudioDevices {
        pub default_input: Option<String>,
        pub default_output: Option<String>,
        pub inputs: Vec<String>,
        pub outputs: Vec<String>,
    }

    pub fn enumerate() -> Result<AudioDevices, cpal::Error> {
        let host = cpal::default_host();
        let default_input = host.default_input_device().map(name);
        let default_output = host.default_output_device().map(name);
        let inputs = names(host.input_devices()?);
        let outputs = names(host.output_devices()?);

        Ok(AudioDevices {
            default_input,
            default_output,
            inputs,
            outputs,
        })
    }

    fn name(device: cpal::Device) -> String {
        device.to_string()
    }

    fn names<I>(devices: I) -> Vec<String>
    where
        I: IntoIterator<Item = cpal::Device>,
    {
        devices
            .into_iter()
            .map(|device| match device.id() {
                Ok(id) => format!("{device}  [id={id}]"),
                Err(_) => name(device),
            })
            .collect()
    }
}

#[cfg(all(feature = "sdk-evaluation", feature = "audio-device-probe"))]
pub mod device_inventory;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureState {
    Idle,
    Requested,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureGate {
    state: CaptureState,
}

impl Default for CaptureGate {
    fn default() -> Self {
        Self {
            state: CaptureState::Idle,
        }
    }
}

impl CaptureGate {
    pub fn state(self) -> CaptureState {
        self.state
    }

    /// Only a user-facing action may move the client into capture setup.
    pub fn request_microphone(&mut self) {
        self.state = CaptureState::Requested;
    }
}

pub struct ClientControlPlane {
    events: BoundedQueue<String>,
}

impl ClientControlPlane {
    pub fn new(event_capacity: usize) -> Self {
        Self {
            events: BoundedQueue::new(event_capacity).expect("configured capacity is non-zero"),
        }
    }

    pub fn push_event(&mut self, event: impl Into<String>) {
        self.events.push_latest(event.into());
    }

    pub fn next_event(&mut self) -> Option<String> {
        self.events.pop()
    }
}

#[cfg(feature = "sdk-evaluation")]
pub fn build_sdk_runtime() -> shared_runtime::Runtime {
    use shared_runtime::{Runtime, RuntimeConfig};
    use std::time::Duration;

    let builder = Runtime::builder(RuntimeConfig {
        max_concurrent_operations: 2,
        max_queued_operations: 4,
        max_concurrent_projections: 2,
        max_queued_projections: 4,
        default_deadline: Duration::from_millis(250),
        shutdown_grace: Duration::from_millis(250),
        cache_budget_bytes: 64 * 1024,
    })
    .expect("fixed runtime limits are valid");
    #[cfg(feature = "audio-device-probe")]
    let builder = device_inventory::register(builder, devices::enumerate);
    builder.build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_retries_within_the_timeout_and_ignores_early_audio() {
        use club_voice_protocol::media::{HEADER, Header, MAX_DATAGRAM};
        use std::{
            net::{Ipv4Addr, UdpSocket},
            thread,
            time::Duration,
        };

        let server = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        server
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let client = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        client.connect(server.local_addr().unwrap()).unwrap();
        let timeout = Some(Duration::from_millis(750));
        client.set_read_timeout(timeout).unwrap();
        thread::scope(|scope| {
            scope.spawn(|| {
                let mut request = [0; 64];
                let (n, peer) = server.recv_from(&mut request).unwrap();
                assert_eq!(&request[..n], b"CV2 JOIN\0Player\0test");
                // Drop the first acknowledgement and send audio before the second.
                let mut early = [0; MAX_DATAGRAM];
                Header {
                    stream: 9,
                    frame_ms: 20,
                    sequence: 0,
                    sent_us: 0,
                }
                .write(&mut early);
                server.send_to(&early[..HEADER + 1], peer).unwrap();
                let (n, again) = server.recv_from(&mut request).unwrap();
                assert_eq!(again, peer);
                assert_eq!(&request[..n], b"CV2 JOIN\0Player\0test");
                let mut ack = *b"CV2A00000000";
                ack[4..].copy_from_slice(&7u64.to_be_bytes());
                server.send_to(&ack, peer).unwrap();
            });
            assert_eq!(join_loopback_voice(&client, "Player").unwrap(), 7);
            assert_eq!(client.read_timeout().unwrap(), timeout);
        });
    }

    #[test]
    fn does_not_request_capture_by_default() {
        assert_eq!(CaptureGate::default().state(), CaptureState::Idle);
    }

    #[test]
    fn capture_requires_an_explicit_request() {
        let mut gate = CaptureGate::default();
        gate.request_microphone();

        assert_eq!(gate.state(), CaptureState::Requested);
    }

    #[cfg(feature = "audio-device-probe")]
    #[test]
    fn device_probe_type_does_not_create_a_capture_gate() {
        let gate = CaptureGate::default();
        assert_eq!(gate.state(), CaptureState::Idle);
    }

    #[cfg(feature = "sdk-evaluation")]
    #[test]
    fn creates_a_bounded_sdk_runtime_for_non_audio_state() {
        assert_eq!(build_sdk_runtime().config().cache_budget_bytes, 64 * 1024);
    }
}
