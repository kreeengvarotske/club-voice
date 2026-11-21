//! Repeatable local control-path smoke test with two independent client sockets.
use club_voice_server::{ServerConfig, loopback::LocalHost};
use std::{
    io,
    net::{SocketAddr, UdpSocket},
    time::Duration,
};

fn exchange(
    socket: &UdpSocket,
    host: SocketAddr,
    message: &[u8],
    expected: &[u8],
) -> io::Result<()> {
    socket.connect(host)?;
    socket.send(message)?;
    let mut response = [0; 32];
    let n = socket.recv(&mut response)?;
    assert_eq!(&response[..n], expected);
    Ok(())
}

fn main() -> io::Result<()> {
    let mut host = LocalHost::bind(
        0,
        ServerConfig {
            name: "Smoke".into(),
            password: "test".into(),
            member_limit: 2,
        },
    )?;
    let addr = host.address()?;
    let server = std::thread::spawn(move || -> io::Result<()> {
        for _ in 0..8 {
            host.serve_one()?;
        }
        Ok(())
    });
    let sockets = [
        UdpSocket::bind("127.0.0.1:0")?,
        UdpSocket::bind("127.0.0.1:0")?,
        UdpSocket::bind("127.0.0.1:0")?,
    ];
    for socket in &sockets {
        socket.set_read_timeout(Some(Duration::from_secs(3)))?;
    }
    for (i, request, reply) in [
        (0, &b"CV1 JOIN\0Ada\0bad"[..], &b"DENIED"[..]),
        (0, b"CV1 JOIN\0Ada\0test", b"LOBBY"),
        (0, b"CV1 JOIN\0Ada\0test", b"LOBBY"),
        (1, b"CV1 JOIN\0Lin\0test", b"LOBBY"),
        (2, b"CV1 JOIN\0Kai\0test", b"FULL"),
        (0, b"CV1 LEAVE", b"LEFT"),
        (0, b"CV1 PING", b"JOIN_REQUIRED"),
        (2, b"CV1 JOIN\0Kai\0test", b"LOBBY"),
    ] {
        exchange(&sockets[i], addr, request, reply)?;
    }
    server.join().expect("server thread panicked")?;
    println!("Loopback admission, duplicate join, capacity and departure passed.");
    Ok(())
}
