// Copyright (c) 2013-2015 Sandstorm Development Group, Inc. and contributors
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
extern crate gjmio;

use std::net::ToSocketAddrs;
use gjmio::{AsyncRead, AsyncWrite, Error, tcp};
use gj::{EventLoop, Promise};

fn forward<R,W,B>(src: R, dst: W, buf: B) -> Promise<(R,W,B), Error<(R,W,B)>>
    where R: AsyncRead, W: AsyncWrite, B: AsMut<[u8]> + AsRef<[u8]> + 'static
{
    src.try_read(buf, 1).then_else(move |r| match r {
        Ok((src, buf, n)) => {
            if n == 0 {
                // EOF
                Promise::ok((src, dst, buf))
            } else {
                dst.write(gjmio::Slice::new(buf, n)).then_else(move |r| match r {
                    Ok((dst, slice)) => forward(src, dst, slice.buf),
                    Err(Error {state: (dst, slice), error}) =>
                        Promise::err(gjmio::Error::new((src, dst, slice.buf), error))
                })
            }
        }
        Err(Error {state: (src, buf), error}) =>
            Promise::err(Error::new((src, dst, buf), error)),
    })
}

fn accept_loop(receiver: tcp::Listener,
               outbound_addr: ::std::net::SocketAddr,
               mut task_set: gj::TaskSet<(), ::std::io::Error>)
               -> Promise<(), ::std::io::Error>
{
    receiver.accept().lift().then(move |(receiver, src_stream)| {
        println!("handling connection");

        gjmio::Timer.timeout_after(::std::time::Duration::from_secs(3),
                                   tcp::Stream::connect(outbound_addr))
           .then_else(move |r| match r {
               Ok(dst_stream) =>  {
                   let (src_reader, src_writer) = src_stream.split();
                   let (dst_reader, dst_writer) = dst_stream.split();
                   task_set.add(forward(src_reader, dst_writer,
                                        vec![0; 1024]).lift().map(|_| Ok(())));
                   task_set.add(forward(dst_reader, src_writer, vec![0; 1024]).lift().map(|_| Ok(())));
                   accept_loop(receiver, outbound_addr, task_set)
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
    let args : Vec<String> = ::std::env::args().collect();
    if args.len() != 3 {
        println!("usage: {} <LISTEN_ADDR> <CONNECT_ADDR>", args[0]);
        return;
    }

    EventLoop::top_level(|wait_scope| -> Result<(), Box<::std::error::Error>> {
        let mut event_port = gjmio::EventPort::new().unwrap();
        let addr = try!(args[1].to_socket_addrs()).next().expect("could not parse address");
        let listener = try!(tcp::Listener::bind(addr));

        let outbound_addr = try!(args[2].to_socket_addrs()).next().expect("could not parse address");
        accept_loop(listener, outbound_addr,
                    gj::TaskSet::new(Box::new(Reporter))).lift().wait(wait_scope, &mut event_port)
    }).unwrap();
}
