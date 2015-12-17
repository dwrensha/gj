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

fn forward<R,W,B>(src: R, dst: W, buf: B) -> gj::Promise<(R,W,B), gj::io::Error<(R,W,B)>>
    where R: AsyncRead, W: AsyncWrite, B: AsMut<[u8]> + AsRef<[u8]> + 'static
{
    src.try_read(buf, 1).then_else(move |r| match r {
        Ok((src, buf, n)) => {
            if n == 0 {
                // EOF
                Ok(gj::Promise::fulfilled((src, dst, buf)))
            } else {
                Ok(dst.write(gj::io::Slice::new(buf, n)).then_else(move |r| match r {
                    Ok((dst, slice)) => Ok(forward(src, dst, slice.buf)),
                    Err(gj::io::Error {state: (dst, slice), error}) =>
                        Err(gj::io::Error::new((src, dst, slice.buf), error))
                }))
            }
        }
        Err(gj::io::Error {state: (src, buf), error}) =>
            Err(gj::io::Error::new((src, dst, buf), error)),
    })
}

fn accept_loop(receiver: gj::io::tcp::Listener,
               outbound_addr: ::std::net::SocketAddr,
               mut task_set: gj::TaskSet<(), ::std::io::Error>) -> gj::Promise<(), ::std::io::Error> {
    receiver.accept().lift().then(move |(receiver, src_stream)| {
        println!("handling connection");

        Ok(gj::io::Timer.timeout_after_ms(3000, ::gj::io::tcp::Stream::connect(outbound_addr))
           .then_else(move |r| match r {
               Ok(dst_stream) =>  {
                   task_set.add(forward(try!(src_stream.try_clone()),
                                        try!(dst_stream.try_clone()),
                                        vec![0; 1024]).lift().map(|_| Ok(())));
                   task_set.add(forward(dst_stream, src_stream, vec![0; 1024]).lift().map(|_| Ok(())));
                   Ok(accept_loop(receiver, outbound_addr, task_set))
               }
               Err(e) => {
                   println!("failed to connect: {}", e);
                   Err(e.into())
               }
           }))
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

    gj::EventLoop::top_level(|wait_scope| {
        let addr = try!(args[1].to_socket_addrs()).next().expect("could not parse address");
        let listener = try!(::gj::io::tcp::Listener::bind(addr));

        let outbound_addr = try!(args[2].to_socket_addrs()).next().expect("could not parse address");
        accept_loop(listener, outbound_addr,
                    gj::TaskSet::new(Box::new(Reporter))).lift().wait(wait_scope)
    }).unwrap();
}
