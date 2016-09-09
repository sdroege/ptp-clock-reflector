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

#[macro_use]
extern crate futures;
#[macro_use]
extern crate tokio_core;

use std::io;
use futures::*;
use tokio_core::reactor::*;
use tokio_core::net::UdpSocket;

use std::net::{SocketAddr, Ipv4Addr, SocketAddrV4};
use std::ops::Range;

struct PtpReflector;

struct PtpSocket {
    socket: UdpSocket,
    addr: SocketAddr,
    handle: Handle,
    buf: [u8; 1024],
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

impl PtpSocket {
    fn new(port: u16, handle: Handle) -> Result<PtpSocket, String> {
        let any_addr = Ipv4Addr::new(0, 0, 0, 0);
        let multicast_group = Ipv4Addr::new(224, 0, 1, 129);
        let bind_addr = SocketAddr::V4(SocketAddrV4::new(any_addr, port));

        let socket = try!(UdpSocket::bind(&bind_addr, &handle).map_err(|e| e.to_string()));
        try!(socket.join_multicast_v4(&multicast_group, &any_addr)
            .map_err(|e| e.to_string()));

        let addr = SocketAddr::V4(SocketAddrV4::new(multicast_group, port));

        Ok(PtpSocket {
            socket: socket,
            addr: addr,
            handle: handle,
            buf: [0; 1024],
        })
    }

    fn handle_packet(&mut self, length: usize, addr: &SocketAddr) {
        println!("Got message of size {}", length);

        if length < 44 {
            return;
        }

        let msg = &mut self.buf;

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
            if let Err(e) = self.socket.send_to(msg, addr) {
                println!("Error while sending message: {}", e.to_string());
                return;
            };

            println!("Sent message of size {}", length);
        }
    }
}

impl Future for PtpSocket {
    type Item = ();
    type Error = io::Error;

    fn poll(&mut self) -> Poll<(), io::Error> {
        loop {
            let (n, addr) = try_nb!(self.socket.recv_from(&mut self.buf));

            println!("read {:?}", self.addr);
            self.handle_packet(n, &addr);
        }
    }
}

impl PtpReflector {
    fn run() -> Result<(), String> {
        let mut l = try!(Core::new().map_err(|e| e.to_string()));

        let event_socket = try!(PtpSocket::new(319, l.handle()).map_err(|e| e.to_string()));
        let general_socket = try!(PtpSocket::new(320, l.handle()).map_err(|e| e.to_string()));

        l.run(event_socket.select(general_socket)).map(|_| ()).map_err(|e| e.0.to_string())
    }
}

fn main() {
    PtpReflector::run().unwrap();
}
