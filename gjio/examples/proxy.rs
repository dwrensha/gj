// Copyright (c) 2013-2016 Sandstorm Development Group, Inc. and contributors
// Licensed under the MIT License:
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
// THE SOFTWARE.

#[macro_use]
extern crate gj;
extern crate gjio;

use std::net::ToSocketAddrs;
use gjio::{AsyncRead, AsyncWrite};
use gj::{EventLoop, Promise};

fn forward<R,W,B>(mut src: R, mut dst: W, buf: B) -> Promise<(), ::std::io::Error>
    where R: AsyncRead + 'static, W: AsyncWrite + 'static, B: AsMut<[u8]> + AsRef<[u8]> + 'static
{
    src.try_read(buf, 1).then(move |(buf, n)| {
        if n == 0 { // EOF
            Promise::ok(())
        } else {
            dst.write(gjio::Slice::new(buf, n)).then(move |slice| {
                forward(src, dst, slice.buf)
            })
        }
    })
}

fn accept_loop(mut receiver: gjio::SocketListener,
               outbound_addr: gjio::SocketAddress,
               timer: gjio::Timer,
               mut task_set: gj::TaskSet<(), ::std::io::Error>)
               -> Promise<(), ::std::io::Error>
{
    receiver.accept().then(move |src_stream| {
        println!("handling connection");

        timer.timeout_after(::std::time::Duration::from_secs(3), outbound_addr.connect())
           .then_else(move |r| match r {
               Ok(dst_stream) =>  {
                   task_set.add(forward(src_stream.clone(), dst_stream.clone(), vec![0; 1024]));
                   task_set.add(forward(dst_stream.clone(), src_stream.clone(), vec![0; 1024]));
                   accept_loop(receiver, outbound_addr, timer, task_set)
               }
               Err(e) => {
                   println!("failed to connect: {}", e);
                   Promise::err(e.into())
               }
           })
    })
}

pub struct Reporter;

impl gj::TaskReaper<(), ::std::io::Error> for Reporter {
    fn task_failed(&mut self, error: ::std::io::Error) {
        println!("Task failed: {}", error);
    }
}

pub fn main() {
    let args: Vec<String> = ::std::env::args().collect();
    if args.len() != 3 {
        println!("usage: {} <LISTEN_ADDR> <CONNECT_ADDR>", args[0]);
        return;
    }

    EventLoop::top_level(|wait_scope| -> Result<(), ::std::io::Error> {
        let mut event_port = try!(gjio::EventPort::new());
        let network = event_port.get_network();
        let addr = try!(args[1].to_socket_addrs()).next().expect("could not parse address");
        let mut address = network.get_tcp_address(addr);
        let listener = try!(address.listen());

        let outbound_addr = try!(args[2].to_socket_addrs()).next().expect("could not parse address");
        let outbound_address = network.get_tcp_address(outbound_addr);
        accept_loop(listener, outbound_address, event_port.get_timer(),
                    gj::TaskSet::new(Box::new(Reporter))).wait(wait_scope, &mut event_port)
    }).unwrap();
}
