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
use gjio::{AsyncRead, AsyncWrite, Slice};
use std::cell::RefCell;
use std::rc::Rc;

/// Container for buffers that are not currently being used on a connection.
struct BufferPool {
    buffers: Vec<Vec<u8>>,
    waiting: Option<PromiseFulfiller<Buffer, ()>>,
}

struct Buffer {
    buf: Vec<u8>,
    pool: Rc<RefCell<BufferPool>>,
}

impl Buffer {
    fn new(buf: Vec<u8>, pool: Rc<RefCell<BufferPool>>) -> Buffer {
        Buffer { buf: buf, pool: pool }
    }
}

impl AsRef<[u8]> for Buffer {
    fn as_ref<'a>(&'a self) -> &'a [u8] {
        &self.buf[..]
    }
}

impl AsMut<[u8]> for Buffer {
    fn as_mut<'a>(&'a mut self) -> &'a mut [u8] {
        &mut self.buf[..]
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        let vec = ::std::mem::replace(&mut self.buf, Vec::with_capacity(0));
        let waiting = self.pool.borrow_mut().waiting.take();
        match waiting {
            Some(fulfiller) => fulfiller.fulfill(Buffer::new(vec, self.pool.clone())),
            None => self.pool.borrow_mut().buffers.push(vec),
        }
    }
}

impl BufferPool {
    pub fn new(buf_size: usize, num_buffers: usize) -> BufferPool {
        BufferPool { buffers: vec![vec![0; buf_size]; num_buffers], waiting: None }
    }

    /// Retrieves a buffer from the pool, waiting until one is available if there are none
    /// already available. Fails if another task is already waiting for a buffer.
    pub fn pop(pool: &Rc<RefCell<BufferPool>>) -> Promise<Buffer, ()> {
        let maybe_buf = pool.borrow_mut().buffers.pop();
        match maybe_buf {
            Some(buf) => Promise::ok(Buffer::new(buf, pool.clone())),
            None => {
                if pool.borrow().waiting.is_some() {
                    Promise::err(())
                } else {
                    let (promise, fulfiller) = Promise::and_fulfiller();
                    pool.borrow_mut().waiting = Some(fulfiller);
                    promise
                }
            }
        }
    }
}

/// Reads `buf`-sized chunks of bytes from a stream until end-of-file, immediately writing each
/// chunk back to the same stream. Note that this function is recursive. In a naive implementation
/// of promises, such a function could potentially create an unbounded chain of promises. However,
/// GJ implements a tail-call optimization that shortens promise chains when possible, and therefore
/// this loop can run indefinitely, consuming only a small, bounded amount of memory.
fn echo(mut stream: gjio::SocketStream, buf: Buffer) -> Promise<(), ::std::io::Error> {
    stream.try_read(buf, 1).then(move |(buf, n)| {
        if n == 0 { // EOF
            Promise::ok(())
        } else {
            stream.write(Slice::new(buf, n)).then(move |slice| {
                echo(stream, slice.buf)
            })
        }
    })
}

/// The task reaper is responsible for returning buffers to the pool once tasks are done with them.
struct Reaper;

impl TaskReaper<(), ::std::io::Error> for Reaper {
    fn task_failed(&mut self, error: ::std::io::Error) {
        println!("Task failed: {}", error);
    }
}

/// Waits for a buffer from the pool, accepts a connection, then spawns an echo() task on that
/// connection with that buffer.
fn accept_loop(mut listener: gjio::SocketListener,
               mut task_set: TaskSet<(), ::std::io::Error>,
               buffer_pool: Rc<RefCell<BufferPool>>)
               -> Promise<(), ::std::io::Error>
{
    let buf_promise = BufferPool::pop(&buffer_pool).map_err(|()| unreachable!());
    buf_promise.then(move |buf| {
        listener.accept().then(move |stream| {
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
        let mut event_port = try!(gjio::EventPort::new());
        let network = event_port.get_network();
        let addr = try!(args[1].to_socket_addrs()).next().expect("could not parse address");
        let mut address = network.get_tcp_address(addr);
        let listener = try!(address.listen());
        let reaper = Box::new(Reaper);
        accept_loop(listener, TaskSet::new(reaper), buffer_pool).lift().wait(wait_scope, &mut event_port)
    }).expect("top level");
}
