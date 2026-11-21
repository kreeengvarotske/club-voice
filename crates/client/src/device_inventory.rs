//! Shared, short-lived metadata for the device picker and settings screen.
use crate::devices::AudioDevices;
use shared_runtime::{
    CachePolicy, CallOptions, Caller, OpContext, Operation, OperationConfig, Runtime,
    RuntimeBuilder,
};
use std::{sync::Arc, time::Duration};

type Enumerate = fn() -> Result<AudioDevices, cpal::Error>;
struct Inventory(Enumerate);

impl Operation for Inventory {
    const NAME: &'static str = "audio.devices";
    type Params = ();
    type Output = AudioDevices;
    type Error = String;
    fn policy(&self) -> CachePolicy {
        CachePolicy::Ttl(Duration::from_secs(2))
    }
    fn weight(&self, devices: &AudioDevices) -> usize {
        std::mem::size_of::<AudioDevices>()
            + (devices.inputs.capacity() + devices.outputs.capacity())
                * std::mem::size_of::<String>()
            + devices
                .inputs
                .iter()
                .chain(&devices.outputs)
                .chain(&devices.default_input)
                .chain(&devices.default_output)
                .map(String::capacity)
                .sum::<usize>()
    }
    async fn run(&self, _: OpContext, _: ()) -> Result<AudioDevices, String> {
        tokio::task::spawn_blocking(self.0)
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())
    }
}

pub(crate) fn register(builder: RuntimeBuilder, enumerate: Enumerate) -> RuntimeBuilder {
    builder
        .register(
            Inventory(enumerate),
            OperationConfig {
                cache_budget_bytes: 32 * 1024,
            },
        )
        .expect("device inventory fits runtime budget")
}

pub async fn snapshot(runtime: &Runtime, caller: &Caller) -> Result<Arc<AudioDevices>, String> {
    runtime
        .operation::<Inventory>()
        .map_err(|e| e.to_string())?
        .get(
            caller,
            (),
            CallOptions {
                deadline: Some(Duration::from_secs(5)),
            },
        )
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    #[tokio::test]
    async fn two_consumers_share_one_device_enumeration() {
        static CALLS: AtomicUsize = AtomicUsize::new(0);
        let config = crate::build_sdk_runtime().config().clone();
        let runtime = register(Runtime::builder(config).unwrap(), || {
            CALLS.fetch_add(1, Ordering::Relaxed);
            Ok(AudioDevices {
                inputs: vec!["Headset".into()],
                outputs: vec![],
                default_input: None,
                default_output: None,
            })
        })
        .build();
        let picker = Caller::window("picker");
        let settings = Caller::window("settings");
        let (a, b) = tokio::join!(snapshot(&runtime, &picker), snapshot(&runtime, &settings));
        assert!(Arc::ptr_eq(&a.unwrap(), &b.unwrap()));
        assert_eq!(CALLS.load(Ordering::Relaxed), 1);
        runtime.shutdown().await;
    }
}
