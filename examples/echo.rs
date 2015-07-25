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

fn echo<S,B>(stream: S, buf: B) -> gj::Promise<()>
    where S: AsyncRead + AsyncWrite, B: ::std::ops::DerefMut<Target=[u8]> + 'static
{
    return stream.try_read(buf, 1).then(move |(stream, buf, n)| {
        if n == 0 {
            // EOF
            return Ok(gj::Promise::fulfilled(()));
        } else {
            return Ok(stream.write(gj::io::Slice::new(buf, n)).then(move |(stream, slice)| {
                return Ok(echo(stream, slice.buf));
            }));
        }
    });
}

fn accept_loop(receiver: gj::io::TcpListener,
               mut task_set: gj::TaskSet) -> gj::Promise<()> {

    return receiver.accept().then(move |(receiver, stream)| {
        task_set.add(echo(stream, vec![0; 1024]));
        return Ok(accept_loop(receiver, task_set));
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
    if args.len() != 2 {
        println!("usage: {} HOST:PORT", args[0]);
        return;
    }

    gj::EventLoop::top_level(move |wait_scope| {
        let addr = try!(args[1].to_socket_addrs()).next().expect("could not parse address");
        let listener = try!(::gj::io::TcpListener::bind(addr));
        return accept_loop(listener,
                           gj::TaskSet::new(Box::new(Reporter))).wait(wait_scope);
    }).unwrap();
}
