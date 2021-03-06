// Copyright (C) 2015 Sebastian Dröge <sebastian@centricular.com>
//
// This library is free software; you can redistribute it and/or
// modify it under the terms of the GNU Library General Public
// License as published by the Free Software Foundation; either
// version 2 of the License, or (at your option) any later version.
//
// This library is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU
// Library General Public License for more details.
//
// You should have received a copy of the GNU Library General Public
// License along with this library; if not, write to the
// Free Software Foundation, Inc., 51 Franklin St, Fifth Floor,
// Boston, MA 02110-1301, USA.
//

extern crate mio;

use mio::*;
use mio::net::*;
use std::net::SocketAddr;
use std::ops::Range;

const SOCKET_EVENT: Token = Token(0);
const SOCKET_GENERAL: Token = Token(1);

struct PtpReflector {
    poll: Poll,
    event_socket: UdpSocket,
    event_addr: SocketAddr,
    general_socket: UdpSocket,
    general_addr: SocketAddr,
}

fn slice_map_range_in_place<T, F>(s: &mut [T], range: Range<usize>, mut f: F)
    where F: FnMut(&T) -> T
{
    let n = s.len();

    if range.end > n {
        panic!("Invalid range {} > {}", range.end, n);
    }

    for i in range {
        s[i] = f(&s[i]);
    }
}

impl PtpReflector {
    fn new() -> Result<PtpReflector, String> {
        let poll = try!(Poll::new().map_err(|e| e.to_string()));

        let multicast_group = "224.0.1.129".parse().unwrap();
        let any_addr = "0.0.0.0".parse().unwrap();

        let event_bind_addr = "0.0.0.0:319".parse().unwrap();
        let event_socket = try!(UdpSocket::bind(&event_bind_addr).map_err(|e| e.to_string()));
        try!(event_socket
                 .join_multicast_v4(&multicast_group, &any_addr)
                 .map_err(|e| e.to_string()));
        try!(poll.register(&event_socket,
                           SOCKET_EVENT,
                           Ready::readable(),
                           PollOpt::level())
                 .map_err(|e| e.to_string()));

        // FIXME: How can we create this from the multicast group?
        // let event_addr = SocketAddr::new(multicast_group, 319);
        let event_addr = "224.0.1.129:319".parse().unwrap();

        let general_bind_addr = "0.0.0.0:320".parse().unwrap();
        let general_socket = try!(UdpSocket::bind(&general_bind_addr).map_err(|e| e.to_string()));
        try!(general_socket
                 .join_multicast_v4(&multicast_group, &any_addr)
                 .map_err(|e| e.to_string()));
        try!(poll.register(&general_socket,
                           SOCKET_GENERAL,
                           Ready::readable(),
                           PollOpt::level())
                 .map_err(|e| e.to_string()));

        // FIXME: How can we create this from the multicast group?
        // let general_addr = SocketAddr::new(multicast_group, 320);
        let general_addr = "224.0.1.129:320".parse().unwrap();

        Ok(PtpReflector {
               poll: poll,
               event_socket: event_socket,
               event_addr: event_addr,
               general_socket: general_socket,
               general_addr: general_addr,
           })
    }

    fn ready(&self, event: &Event) {
        if !event.readiness().is_readable() {
            return;
        }

        let (socket, addr) = match event.token() {
            SOCKET_EVENT => (&self.event_socket, &self.event_addr),
            SOCKET_GENERAL => (&self.general_socket, &self.general_addr),
            _ => unreachable!(),
        };

        let msg = &mut [0; 1024];

        let length = match socket.recv_from(msg) {
            Err(e) => {
                println!("Error while receiving message: {}", e.to_string());
                return;
            }
            Ok((l, _)) => l,
        };

        println!("Got message of size {}", length);

        if length < 44 {
            return;
        }

        let domain = msg[4];
        let msg_type = msg[0] & 0x0f;

        let forward = match domain {
            0 => {
                msg[4] = 1;
                slice_map_range_in_place(msg, 20..28, |&x| x ^ 0xff);

                match msg_type {
                    0x0 | 0x08 | 0xb => true,
                    0x9 if msg.len() >= 54 => {
                        slice_map_range_in_place(msg, 44..52, |&x| x ^ 0xff);

                        true
                    }
                    _ => false,
                }
            }
            1 => {
                msg[4] = 0;
                slice_map_range_in_place(msg, 20..28, |&x| x ^ 0xff);

                match msg_type {
                    0x1 => true,
                    _ => false,
                }
            }
            _ => false,
        };

        if forward {
            if let Err(e) = socket.send_to(&msg[..length], addr) {
                println!("Error while sending message: {}", e.to_string());
                return;
            };

            println!("Sent message of size {}", length);
        }
    }

    fn run(&self) -> Result<(), String> {
        let mut events = Events::with_capacity(1024);

        loop {
            try!(self.poll
                     .poll(&mut events, None)
                     .or_else(|err| Err(err.to_string())));

            for event in events.iter() {
                self.ready(&event);
            }
        }
    }
}

fn main() {
    let res = PtpReflector::new().and_then(|handler| handler.run());

    match res {
        Ok(()) => (),
        Err(s) => panic!(s),
    };
}
