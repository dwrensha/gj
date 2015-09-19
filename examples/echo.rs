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

//! Echo server with a bounded buffer pool.

extern crate gj;
use std::net::ToSocketAddrs;
use std::cell::RefCell;
use std::rc::Rc;
use gj::io::{AsyncRead, AsyncWrite};

struct BufferPool {
    buffers: Vec<Vec<u8>>,
    fulfiller: Option<Box<gj::PromiseFulfiller<Vec<u8>, ()>>>,
}

impl BufferPool {
    pub fn new(num_buffers: usize) -> BufferPool {
        BufferPool { buffers: vec![vec![0; 1024]; num_buffers], fulfiller: None }
    }

    pub fn pop(&mut self) -> gj::Promise<Vec<u8>, ()> {
        match self.buffers.pop() {
            Some(buf) => gj::Promise::fulfilled(buf),
            None => {
                if self.fulfiller.is_some() {
                    gj::Promise::rejected(())
                } else {
                    let (promise, fulfiller) = gj::new_promise_and_fulfiller();
                    self.fulfiller = Some(fulfiller);
                    promise
                }
            }
        }
    }

    pub fn push(&mut self, buf: Vec<u8>) {
        match self.fulfiller.take() {
            Some(fulfiller) => fulfiller.fulfill(buf),
            None => self.buffers.push(buf),
        }
    }
}

struct Reaper {
    buffer_pool: Rc<RefCell<BufferPool>>,
}

impl gj::TaskReaper<ThreadedState, gj::io::Error<ThreadedState>> for Reaper {
    fn task_succeeded(&mut self, (_, buf): ThreadedState) {
        self.buffer_pool.borrow_mut().push(buf);
    }
    fn task_failed(&mut self, error: gj::io::Error<ThreadedState>) {
        self.buffer_pool.borrow_mut().push(error.state.1);
        println!("Task failed: {}", error.error);
    }
}

type ThreadedState = (::gj::io::tcp::Stream, Vec<u8>);

fn echo<S,B>(stream: S, buf: B) -> gj::Promise<(S, B), gj::io::Error<(S, B)>>
    where S: AsyncRead + AsyncWrite, B: ::std::ops::DerefMut<Target=[u8]> + 'static
{
    stream.try_read(buf, 1).then(move |(stream, buf, n)| {
        if n == 0 {
            // EOF
            Ok(gj::Promise::fulfilled((stream, buf)))
        } else {
            Ok(stream.write(gj::io::Slice::new(buf, n)).then_else(move |r| match r {
                Ok((stream, slice)) => Ok(echo(stream, slice.buf)),
                Err(gj::io::Error {state: (stream, slice), error}) =>
                    Err(gj::io::Error::new((stream, slice.buf), error)),
            }))
        }
    })
}

fn accept_loop(listener: gj::io::tcp::Listener,
               mut task_set: gj::TaskSet<ThreadedState, gj::io::Error<ThreadedState>>,
               buffer_pool: Rc<RefCell<BufferPool>>)
               -> gj::Promise<(), ::std::io::Error>
{
    let buf_promise = buffer_pool.borrow_mut().pop().map_err(|()| {
        ::std::io::Error::new(::std::io::ErrorKind::Other, "No available buffers")});
    buf_promise.then(move |buf| {
        Ok(listener.accept().lift().then(move |(listener, stream)| {
            task_set.add(echo(stream, buf));
            Ok(accept_loop(listener, task_set, buffer_pool))
        }))
    })
}

pub fn main() {
    let args: Vec<String> = ::std::env::args().collect();
    if args.len() != 2 {
        println!("usage: {} HOST:PORT", args[0]);
        return;
    }
    let buffer_pool = Rc::new(RefCell::new(BufferPool::new(64)));

    gj::EventLoop::top_level(move |wait_scope| {
        let addr = try!(args[1].to_socket_addrs()).next().expect("could not parse address");
        let listener = try!(gj::io::tcp::Listener::bind(addr));
        let reaper = Box::new(Reaper { buffer_pool: buffer_pool.clone() });
        accept_loop(listener, gj::TaskSet::new(reaper), buffer_pool).lift().wait(wait_scope)
    }).unwrap();
}
