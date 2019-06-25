use std::error::Error;
use std::io;
use std::net::{Ipv4Addr, UdpSocket, SocketAddr, ToSocketAddrs};
use std::str;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

use bincode::{deserialize, serialize};
use log::info;
use rand::prelude::*;
use serde::{Deserialize, Serialize};

use socket2::{Socket, Domain, Type, Protocol};

const MULTICAST_ADDR: Ipv4Addr = Ipv4Addr::new(239, 0, 0, 1);

#[derive(Serialize, Deserialize, Debug)]
enum Message {
    FindLeader,
    AnswerLeader { id: u64 },
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

fn send_message<A: ToSocketAddrs>(socket: &UdpSocket, addr: &A, message: &Message) -> Result<(), Box<dyn Error>> {
    let serialized_msg = serialize(&message)?;
    socket.send_to(&serialized_msg, addr)?;

    Ok(())
}

struct SharedState {
    id: u64,
    socket: UdpSocket,
    rng: ThreadRng,
}

trait BullyState {
    fn execute(self: Box<Self>) -> Result<Box<dyn BullyState>, Box<dyn Error>>;
}

struct AskForLeader {
    shared_state: SharedState,
}

impl BullyState for AskForLeader {
    fn execute(self: Box<Self>) -> Result<Box<dyn BullyState>, Box<dyn Error>> {
        info!("asking leader");

        let ask_msg = Message::FindLeader;

        send_message(&self.shared_state.socket, &(MULTICAST_ADDR, 6000), &ask_msg)?;

        thread::sleep(Duration::from_secs(10));

        let mut buf = [0; 512];

        self.shared_state.socket.recv(&mut buf)?;

        let msg: Message = deserialize(&buf)?;

        match msg {
            Message::AnswerLeader { id } => Ok(Box::new(RunFollowerLogic { leader_id: id, shared_state: self.shared_state })),
            _ => Ok(Box::new(UnknownLeader { shared_state: self.shared_state }))
        }
    }
}

struct UnknownLeader {
    shared_state: SharedState,
}

impl BullyState for UnknownLeader {
    fn execute(self: Box<Self>) -> Result<Box<dyn BullyState>, Box<dyn Error>> {
        info!("sending election msg");
        let call_election_msg = Message::CallElection { id: self.shared_state.id };

        send_message(&self.shared_state.socket, &(MULTICAST_ADDR, 6000), &call_election_msg)?;

        Ok(Box::new(WaitingForChallengeResponse { shared_state: self.shared_state }))
    }
}

struct WaitingForChallengeResponse {
    shared_state: SharedState,
}

impl BullyState for WaitingForChallengeResponse {
    fn execute(self: Box<Self>) -> Result<Box<dyn BullyState>, Box<dyn Error>> {
        info!("waiting for challenge response");

        thread::sleep(Duration::from_secs(10));

        self.shared_state.socket.set_nonblocking(true)?;
        let mut buf = [0; 512];
        let mut msgs = Vec::new();

        loop {
            match self.shared_state.socket.recv(&mut buf) {
                Ok(n) => {
                    let deserialized_msg: Message = deserialize(&buf[..n])?;

                    info!("{:?}", deserialized_msg);

                    match deserialized_msg {
                        Message::AnswerElection { id } if id == self.shared_state.id => (),
                        Message::CallElection { id } if id == self.shared_state.id => (),
                        Message::AnswerElection { id } => msgs.push(id),
                        Message::CallElection { id } => msgs.push(id),
                        _ => info!("{:?}", deserialized_msg),
                    }
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

        info!("{:?}", msgs);

        self.shared_state.socket.set_nonblocking(false)?;

        let max_id = msgs.into_iter()
            .filter(|&id| id > self.shared_state.id)
            .max();

        match max_id {
            None => Ok(Box::new(RunLeaderLogic { shared_state: self.shared_state })),
            Some(max_id) => Ok(Box::new(RunFollowerLogic { leader_id: max_id, shared_state: self.shared_state })),
        }
    }
}

struct RunFollowerLogic {
    leader_id: u64,
    shared_state: SharedState,
}

impl BullyState for RunFollowerLogic {
    fn execute(self: Box<Self>) -> Result<Box<BullyState>, Box<Error>> {
        unimplemented!()
    }
}

struct RunLeaderLogic {
    shared_state: SharedState
}

impl BullyState for RunLeaderLogic {
    fn execute(self: Box<Self>) -> Result<Box<BullyState>, Box<Error>> {
        info!("running leader logic");

        loop {
            let mut buf = [0; 512];

            let bytes_received = self.shared_state.socket.recv(&mut buf)?;

            let deserialized_msg: Message = deserialize(&buf[..bytes_received])?;

            match deserialized_msg {
                Message::CallElection { id: message_id } => {
                    info!("received election call, id {}", message_id);

                    if message_id < self.shared_state.id {
                        let answer_election_msg = Message::AnswerElection { id: self.shared_state.id };
                        send_message(&self.shared_state.socket, &(MULTICAST_ADDR, 6000), &answer_election_msg)?;
                    }

                    return Ok(Box::new(UnknownLeader { shared_state: self.shared_state }))
                },
                Message::FindLeader => {
                    info!("received AskLeader");

                    let answer_msg = Message::AnswerLeader { id: self.shared_state.id };
                    send_message(&self.shared_state.socket, &(MULTICAST_ADDR, 6000), &answer_msg)?;
                }
                _ => panic!("I don't want to deal with this right now: {:?}", deserialized_msg)
            }
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    let mut rng = thread_rng();
    let self_id: u64 = rng.gen();
    let extra_delay = rng.gen_range(0, 1000);
    let time = Arc::new(AtomicUsize::new(1));

    let time_clone = time.clone();

    info!("process extra skew ms is {}", extra_delay);

    info!("process has id {}", self_id);

    let _thread = thread::spawn(move || {
        loop {
            let current_time = time_clone.fetch_add(1, Ordering::SeqCst);
            info!("process has time {}", current_time);

            thread::sleep(Duration::from_secs(1) + Duration::from_millis(extra_delay));
        }
    });

    let multicast_addr = Ipv4Addr::new(239, 0, 0, 1);
    let socket = Socket::new(Domain::ipv4(), Type::dgram(), Some(Protocol::udp()))?;
    socket.join_multicast_v4(&multicast_addr, &Ipv4Addr::UNSPECIFIED)?;
    socket.set_reuse_address(true)?;
    socket.set_multicast_loop_v4(true)?;
    socket.bind(&"0.0.0.0:6000".parse::<SocketAddr>()?.into())?;

    let socket: UdpSocket = socket.into();

    let shared_state = SharedState { socket, id: self_id, rng };

    let mut process_state: Box<dyn BullyState> = Box::new(AskForLeader { shared_state });

    loop {
        process_state = process_state.execute()?;
    }

    Ok(())
}
