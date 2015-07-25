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
use gj::io::{AsyncRead, AsyncWrite};

fn forward<R,W,B>(src: R, dst: W, buf: B) -> gj::Promise<()>
    where R: AsyncRead, W: AsyncWrite, B: ::std::ops::DerefMut<Target=[u8]> + 'static
{
    return src.try_read(buf, 1).then(move |(src, buf, n)| {
        if n == 0 {
            // EOF
            return Ok(gj::Promise::fulfilled(()));
        } else {
            return Ok(dst.write(gj::io::Slice::new(buf, n)).then(move |(dst, slice)| {
                return Ok(forward(src, dst, slice.buf));
            }));
        }
    });
}

fn accept_loop(receiver: gj::io::TcpListener,
               outbound_addr: gj::io::NetworkAddress,
               mut task_set: gj::TaskSet) -> gj::Promise<()> {

    return receiver.accept().then(move |(receiver, src_stream)| {
        println!("handling connection");

        return Ok(gj::io::Timer.timeout_after_ms(1000, outbound_addr.connect())
                .then_else(move |dst_stream| {
                    task_set.add(forward(try!(src_stream.try_clone()),
                                         try!(dst_stream.try_clone()),
                                         vec![0; 1024]));
                    task_set.add(forward(dst_stream, src_stream, vec![0; 1024]));
                    return Ok(accept_loop(receiver, outbound_addr, task_set));
                }, |e| {
                    println!("failed to connect: {}", e);
                    return Err(e);
                }));
    });
}

pub struct Reporter;

impl gj::ErrorHandler for Reporter {
    fn task_failed(&mut self, error: gj::Error) {
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

        let addr = gj::io::NetworkAddress::new(&*args[1]).unwrap();
        let receiver = addr.listen().unwrap();

        let outbound_addr = gj::io::NetworkAddress::new(&*args[2]).unwrap();

        return accept_loop(receiver,
                           outbound_addr,
                           gj::TaskSet::new(Box::new(Reporter))).wait(wait_scope);
    }).unwrap();
}
