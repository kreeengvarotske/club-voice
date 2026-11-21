//! A diagnostic command that only enumerates audio devices.
//!
//! It intentionally requires an explicit `--list-devices` argument and never
//! creates a CPAL stream, so executing it cannot record from a microphone.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().nth(1).as_deref() != Some("--list-devices") {
        eprintln!("Usage: club-voice-device-check --list-devices");
        return Ok(());
    }

    #[cfg(not(feature = "sdk-evaluation"))]
    let devices = club_voice_client::devices::enumerate()?;
    #[cfg(feature = "sdk-evaluation")]
    let devices = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?
        .block_on(async {
            let runtime = club_voice_client::build_sdk_runtime();
            let result = club_voice_client::device_inventory::snapshot(
                &runtime,
                &shared_runtime::Caller::window("device-check"),
            )
            .await;
            runtime.shutdown().await;
            result
        })?;
    println!(
        "Default input: {}",
        devices.default_input.as_deref().unwrap_or("none")
    );
    println!(
        "Default output: {}",
        devices.default_output.as_deref().unwrap_or("none")
    );
    println!("Input devices:");
    for name in &devices.inputs {
        println!("- {name}");
    }
    println!("Output devices:");
    for name in &devices.outputs {
        println!("- {name}");
    }
    Ok(())
}
