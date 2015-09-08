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

extern crate gj;
use std::net::ToSocketAddrs;
use gj::io::{AsyncRead, AsyncWrite};

fn forward<R,W,B>(src: R, dst: W, buf: B) -> gj::Promise<(), ::std::io::Error>
    where R: AsyncRead, W: AsyncWrite, B: ::std::ops::DerefMut<Target=[u8]> + 'static
{
    src.try_read(buf, 1).lift().then(move |(src, buf, n)| {
        if n == 0 {
            // EOF
            Ok(gj::Promise::fulfilled(()))
        } else {
            Ok(dst.write(gj::io::Slice::new(buf, n)).lift().then(move |(dst, slice)| {
                Ok(forward(src, dst, slice.buf))
            }))
        }
    })
}

fn accept_loop(receiver: gj::io::tcp::Listener,
               outbound_addr: ::std::net::SocketAddr,
               mut task_set: gj::TaskSet) -> gj::Promise<(), Box<::std::error::Error>> {
    receiver.accept().lift().then(move |(receiver, src_stream)| {
        println!("handling connection");

        Ok(gj::io::Timer.timeout_after_ms(3000, ::gj::io::tcp::Stream::connect(outbound_addr))
           .then_else(move |r| match r {
               Ok(dst_stream) =>  {
                   task_set.add(forward(try!(src_stream.try_clone()),
                                        try!(dst_stream.try_clone()),
                                        vec![0; 1024]).lift());
                   task_set.add(forward(dst_stream, src_stream, vec![0; 1024]).lift());
                   Ok(accept_loop(receiver, outbound_addr, task_set).lift())
               }
               Err(e) => {
                   println!("failed to connect: {}", e);
                   Err(e.into())
               }
           }))
    })
}

pub struct Reporter;

impl gj::ErrorHandler for Reporter {
    fn task_failed(&mut self, error: Box<::std::error::Error>) {
        println!("Task failed: {}", error);
    }
}

pub fn main() {
    let args : Vec<String> = ::std::env::args().collect();
    if args.len() != 3 {
        println!("usage: {} <LISTEN_ADDR> <CONNECT_ADDR>", args[0]);
        return;
    }

    gj::EventLoop::top_level(|wait_scope| {
        let addr = try!(args[1].to_socket_addrs()).next().expect("could not parse address");
        let listener = try!(::gj::io::tcp::Listener::bind(addr));

        let outbound_addr = try!(args[2].to_socket_addrs()).next().expect("could not parse address");
        return accept_loop(listener,
                           outbound_addr,
                           gj::TaskSet::new(Box::new(Reporter))).wait(wait_scope);
    }).unwrap();
}
