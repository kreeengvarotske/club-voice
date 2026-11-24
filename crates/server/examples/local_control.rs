use club_voice_protocol::{LAN_PORT, parse_lan_address};
use club_voice_server::{ServerConfig, loopback::LocalHost};
use std::io::{self, Write};

fn main() -> io::Result<()> {
    if std::env::args().nth(1).as_deref() == Some("--lan") {
        let input = prompt(&format!(
            "Bind address (private IPv4, default port {LAN_PORT})"
        ))?;
        let address = parse_lan_address(&input)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        let password = prompt("Room password (8–128 bytes)")?;
        let mut host = LocalHost::bind_lan(
            address,
            ServerConfig {
                name: "Club Voice LAN pilot".into(),
                password,
                member_limit: 2,
            },
        )?;
        println!("LAN pilot listening on {}", host.address()?);
        io::stdout().flush()?;
        return host.serve();
    }
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
    host.serve()
}

fn prompt(label: &str) -> io::Result<String> {
    print!("{label}: ");
    io::stdout().flush()?;
    let mut value = String::new();
    if io::stdin().read_line(&mut value)? == 0 {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "input closed"));
    }
    Ok(value.trim().to_owned())
}
