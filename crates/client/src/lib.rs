//! Client-side state that is safe to construct without opening an audio device.

use club_voice_protocol::BoundedQueue;

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
