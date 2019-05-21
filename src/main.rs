use std::error::Error;
use std::net::{Ipv4Addr, UdpSocket};
use std::str;

fn str_from_u8_nul_utf8(utf8_src: &[u8]) -> Result<&str, std::str::Utf8Error> {
    let nul_range_end = utf8_src.iter()
        .position(|&c| c == b'\0')
        .unwrap_or_else(|| utf8_src.len()); // default to length if no `\0` present
    str::from_utf8(&utf8_src[0..nul_range_end])
}

fn main() -> Result<(), Box<dyn Error>> {
    let socket = UdpSocket::bind("0.0.0.0:6000")?;

    let mut buf = [0; 512];

    let multicast_addr = Ipv4Addr::new(239, 0, 0, 1);

    socket.join_multicast_v4(&multicast_addr, &Ipv4Addr::UNSPECIFIED)?;

    loop {
        match socket.recv(&mut buf) {
            Ok(received) => {
                println!("received {} bytes {:?}", received, &buf[..received]);
                let decoded = str_from_u8_nul_utf8(&buf[..received])?;
                println!("message is {:?}", decoded);
            }
            _ => panic!("wtf")
        }
    }
}

