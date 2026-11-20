use club_voice_server::{ServerConfig, loopback::LocalHost};
use std::io::{self, Write};

fn main() -> io::Result<()> {
    let mut host = LocalHost::bind(
        0,
        ServerConfig {
            name: "Local control smoke".into(),
            password: "test".into(),
            member_limit: 2,
        },
    )?;
    println!("{}", host.address()?);
    io::stdout().flush()?;
    loop {
        host.serve_one()?;
    }
}
