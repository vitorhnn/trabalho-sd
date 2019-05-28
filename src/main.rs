use std::error::Error;
use std::net::{Ipv4Addr, UdpSocket};
use std::str;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use std::thread;

use rand::prelude::*;

use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
enum Message {
    CallElection { id: u64  },
    AnswerElection { id: u64 }
}

enum State {
    UnknownLeader,
    WaitingForChallengeResponse,
    RunLeaderLogic,
    RunFollowerLogic { leader_id: u64 },
}

fn str_from_u8_nul_utf8(utf8_src: &[u8]) -> Result<&str, std::str::Utf8Error> {
    let nul_range_end = utf8_src.iter()
        .position(|&c| c == b'\0')
        .unwrap_or_else(|| utf8_src.len()); // default to length if no `\0` present
    str::from_utf8(&utf8_src[0..nul_range_end])
}

fn main() -> Result<(), Box<dyn Error>> {
    let time = Arc::new(AtomicUsize::new(1));

    let time_clone = time.clone();

    let 

    let thread = thread::spawn(move || {
        let current_time = time_clone.fetch_add(1, Ordering::SeqCst);
        println!("current time is {}", current_time);
    });

    let socket = UdpSocket::bind("0.0.0.0:6000")?;

    let mut buf = [0; 512];

    let multicast_addr = Ipv4Addr::new(239, 0, 0, 1);

    socket.join_multicast_v4(&multicast_addr, &Ipv4Addr::UNSPECIFIED)?;

    let id: u64 = random();

    println!("id is {}", id);

    loop {
        match socket.recv(&mut buf) {
            Ok(received) => {
                println!("received {} bytes {:?}", received, &buf[..received]);
                let decoded = str_from_u8_nul_utf8(&buf[..received])?;
                println!("message is {}", decoded);
            }
            _ => panic!("wtf")
        }
    }
}

