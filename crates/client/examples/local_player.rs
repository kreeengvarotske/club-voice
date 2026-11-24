use std::{
    env,
    io::{self, BufRead, Write},
    net::{Ipv4Addr, SocketAddrV4, UdpSocket},
    time::Duration,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let address: SocketAddrV4 = args.next().ok_or("missing host address")?.parse()?;
    let name = args.next().ok_or("missing display name")?;
    if !address.ip().is_loopback()
        || name.contains('\0')
        || name.trim().is_empty()
        || name.len() > 48
    {
        return Err("expected a loopback address and a valid display name".into());
    }
    let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))?;
    socket.connect(address)?;
    socket.set_read_timeout(Some(Duration::from_secs(3)))?;
    let exchange = |request: &[u8]| -> io::Result<String> {
        socket.send(request)?;
        let mut reply = [0; 32];
        let size = socket.recv(&mut reply)?;
        Ok(String::from_utf8_lossy(&reply[..size]).into_owned())
    };
    let reply = exchange(format!("CV1 JOIN\0{name}\0test").as_bytes())?;
    println!("{reply}");
    io::stdout().flush()?;
    if reply != "LOBBY" {
        return Err("join refused".into());
    }
    for line in io::stdin().lock().lines() {
        match line?.as_str() {
            "ping" => println!("{}", exchange(b"CV1 PING")?),
            "leave" => break,
            _ => eprintln!("Expected ping or leave"),
        }
    }
    println!("{}", exchange(b"CV1 LEAVE")?);
    Ok(())
}
