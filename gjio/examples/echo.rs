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

//! Single-threaded TCP echo server with a bounded buffer pool. Allocates N buffers upon
//! initialization and uses them to serve up to N clients concurrently. When all buffers are in use,
//! the server waits until the next buffer becomes available before accepting the next client
//! connection.

extern crate gj;
extern crate gjio;

use gj::{EventLoop, Promise, PromiseFulfiller, TaskReaper, TaskSet};
use std::cell::RefCell;
use std::rc::Rc;

/// Container for buffers that are not currently being used on a connection.
struct BufferPool {
    buffers: Vec<Vec<u8>>,
    waiting: Option<PromiseFulfiller<Vec<u8>, ()>>,
}

impl BufferPool {
    pub fn new(buf_size: usize, num_buffers: usize) -> BufferPool {
        BufferPool { buffers: vec![vec![0; buf_size]; num_buffers], waiting: None }
    }

    /// Retrieves a buffer from the pool, waiting until one is available if there are none
    /// already available. Fails if another task is already waiting for a buffer.
    pub fn pop(&mut self) -> Promise<Vec<u8>, ()> {
        match self.buffers.pop() {
            Some(buf) => Promise::ok(buf),
            None => {
                if self.waiting.is_some() {
                    Promise::err(())
                } else {
                    let (promise, fulfiller) = Promise::and_fulfiller();
                    self.waiting = Some(fulfiller);
                    promise
                }
            }
        }
    }

    /// Returns a buffer to the pool.
    pub fn push(&mut self, buf: Vec<u8>) {
        match self.waiting.take() {
            Some(fulfiller) => fulfiller.fulfill(buf),
            None => self.buffers.push(buf),
        }
    }
}

/// This is the state that gets threaded through the echo loop. When the echo loop is done,
/// the Vec will get returned to the buffer pool.
type TaskState = (tcp::Stream, Vec<u8>);

/// Reads `buf`-sized chunks of bytes from a stream until end-of-file, immediately writing each
/// chunk back to the same stream. Note that this function is recursive. In a naive implementation
/// of promises, such a function could potentially create an unbounded chain of promises. However,
/// GJ implements a tail-call optimization that shortens promise chains when possible, and therefore
/// this loop can run indefinitely, consuming only a small, bounded amount of memory.
fn echo(stream: gjio::SocketStream, buf: Vec<u8>) -> Promise<TaskState, ::std::io::Error> {
    stream.try_read(buf, 1).then(move |(stream, buf, n)| {
        if n == 0 { // EOF
            Promise::ok((stream, buf))
        } else {
            stream.write(Slice::new(buf, n)).then_else(move |r| match r {
                Err(Error {state: (stream, slice), error}) =>
                    Promise::err(Error::new((stream, slice.buf), error)),
                Ok((stream, slice)) => echo(stream, slice.buf),
            })
        }
    })
}

/// The task reaper is responsible for returning buffers to the pool once tasks are done with them.
struct Reaper {
    buffer_pool: Rc<RefCell<BufferPool>>,
}

impl TaskReaper<TaskState, Error<TaskState>> for Reaper {
    fn task_succeeded(&mut self, (_, buf): TaskState) {
        self.buffer_pool.borrow_mut().push(buf);
    }
    fn task_failed(&mut self, error: Error<TaskState>) {
        self.buffer_pool.borrow_mut().push(error.state.1);
        println!("Task failed: {}", error.error);
    }
}

/// Waits for a buffer from the pool, accepts a connection, then spawns an echo() task on that
/// connection with that buffer.
fn accept_loop(listener: tcp::Listener,
               mut task_set: TaskSet<TaskState, Error<TaskState>>,
               buffer_pool: Rc<RefCell<BufferPool>>)
               -> Promise<(), ::std::io::Error>
{
    let buf_promise = buffer_pool.borrow_mut().pop().map_err(|()| unreachable!());
    buf_promise.then(move |buf| {
        listener.accept().lift().then(move |(listener, stream)| {
            task_set.add(echo(stream, buf));
            accept_loop(listener, task_set, buffer_pool)
        })
    })
}

pub fn main() {
    let args: Vec<String> = ::std::env::args().collect();
    if args.len() != 2 {
        println!("usage: {} HOST:PORT", args[0]);
        return;
    }
    let buffer_pool = Rc::new(RefCell::new(BufferPool::new(1024, 64)));

    EventLoop::top_level(move |wait_scope| -> Result<(), ::std::io::Error> {
        use std::net::ToSocketAddrs;
        let mut event_port = try!(gjmio::EventPort::new());
        let addr = try!(args[1].to_socket_addrs()).next().expect("could not parse address");
        let listener = try!(tcp::Listener::bind(addr));
        let reaper = Box::new(Reaper { buffer_pool: buffer_pool.clone() });
        accept_loop(listener, TaskSet::new(reaper), buffer_pool).lift().wait(wait_scope, &mut event_port)
    }).unwrap();
}
