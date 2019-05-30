use std::error::Error;
use std::io;
use std::net::{Ipv4Addr, UdpSocket};
use std::str;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

use bincode::{deserialize, serialize};
use log::info;
use rand::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
enum Message {
    CallElection { id: u64  },
    AnswerElection { id: u64 }
}

#[derive(Debug)]
enum State {
    UnknownLeader,
    WaitingForChallengeResponse,
    RunLeaderLogic,
    RunFollowerLogic { leader_id: u64 },
}

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    let mut rng = thread_rng();
    let id: u64 = rng.gen();
    let extra_delay = rng.gen_range(0, 1000);
    let time = Arc::new(AtomicUsize::new(1));
    let mut process_state = State::UnknownLeader;

    let time_clone = time.clone();

    info!("process extra skew ms is {}", extra_delay);

    info!("process has id {}", id);

    let _thread = thread::spawn(move || {
        loop {
            let current_time = time_clone.fetch_add(1, Ordering::SeqCst);
            info!("process has time {}", current_time);

            thread::sleep(Duration::from_secs(1) + Duration::from_millis(extra_delay));
        }
    });

    let socket = UdpSocket::bind("0.0.0.0:6000")?;

    let multicast_addr = Ipv4Addr::new(239, 0, 0, 1);

    socket.join_multicast_v4(&multicast_addr, &Ipv4Addr::UNSPECIFIED)?;

    loop {
        match process_state {
            State::UnknownLeader => {
                info!("sending election msg");
                let call_election_msg = Message::CallElection { id };
                let serialized_msg = serialize(&call_election_msg)?;
                socket.send_to(&serialized_msg, (multicast_addr, 6000))?;

                process_state = State::WaitingForChallengeResponse;
            },
            State::WaitingForChallengeResponse => {
                info!("waiting for challenge response");

                thread::sleep(Duration::from_secs(10));

                socket.set_nonblocking(true)?;
                let mut buf = [0; 512];
                let mut msgs = Vec::new();

                loop {
                    match socket.recv(&mut buf) {
                        Ok(n) => {
                            let deserialized_msg: Message = deserialize(&buf[..n])?;
                            if let Message::AnswerElection { id } = deserialized_msg {
                                msgs.push(id);
                            }
                        },
                        Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                            break;
                        }
                        _ => panic!("error while looping for challenge responses")
                    }
                };

                socket.set_nonblocking(false)?;

                let max_id = msgs.iter()
                    .max();

                if let None = max_id {
                    process_state = State::RunLeaderLogic;
                }

                if let Some(max_id) = max_id {
                    assert!(*max_id > id, "received challenge response with id smaller than local id");

                    process_state = State::RunFollowerLogic { leader_id: *max_id };
                }
            },
            State::RunFollowerLogic { leader_id } => {
                info!("running follower logic with leader_id {}", leader_id);
            },
            State::RunLeaderLogic => {
                info!("running leader logic");

                loop {
                    let mut buf = [0; 512];

                    let bytes_received = socket.recv(&mut buf)?;

                    let deserialized_msg: Message = deserialize(&buf[..bytes_received])?;

                    match deserialized_msg {
                        Message::CallElection { id: message_id } => {
                            let answer_election_msg = Message::AnswerElection { id };
                            let serialized_msg = serialize(&answer_election_msg)?;
                            socket.send_to(&serialized_msg, (multicast_addr, 6000))?;
                        },
                        _ => panic!("I don't want to deal with this right now: {:?}", deserialized_msg)
                    }
                }
            },
            _ => panic!("unimplemented state {:?}", process_state)
        }
    }
}
