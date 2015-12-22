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

//! Asynchronous input and output.

use std::result::Result;
use handle_table::{HandleTable, Handle};
use {EventPort, Promise, PromiseFulfiller, new_promise_and_fulfiller};
use private::{with_current_event_loop};

pub mod tcp;

#[cfg(unix)]
pub mod unix;

pub struct Error<S> {
    pub state: S,
    pub error: ::std::io::Error,
}

impl <S> Error<S> {
    pub fn new(state: S, error: ::std::io::Error) -> Error<S> {
        Error { state: state, error: error }
    }
}

impl <S> Into<::std::io::Error> for Error<S> {
    fn into(self) -> ::std::io::Error {
        self.error
    }
}

impl <S> Into<Box<::std::error::Error>> for Error<S> {
    fn into(self) -> Box<::std::error::Error> {
        self.error.into()
    }
}

/// A nonblocking input bytestream.
pub trait AsyncRead: 'static {
    /// Attempts to read `buf.len()` bytes from the stream, writing them into `buf`.
    /// Returns `self`, the modified `buf`, and the number of bytes actually read.
    /// Returns as soon as `min_bytes` are read or EOF is encountered.
    fn try_read<T>(self, buf: T, min_bytes: usize) -> Promise<(Self, T, usize), Error<(Self, T)>>
        where T: AsMut<[u8]>, Self: Sized;

    /// Like `try_read()`, but returns an error if EOF is encountered before `min_bytes`
    /// can be read.
    fn read<T>(self, buf: T, min_bytes: usize) -> Promise<(Self, T, usize), Error<(Self, T)>>
        where T: AsMut<[u8]>, Self: Sized
    {
        self.try_read(buf, min_bytes).map(move |(s, buf, n)| {
            if n < min_bytes {
                Err(Error::new((s, buf),
                               ::std::io::Error::new(::std::io::ErrorKind::Other, "Premature EOF")))
            } else {
                Ok((s, buf, n))
            }
        })
    }
}

/// A nonblocking output bytestream.
pub trait AsyncWrite: 'static {
    /// Attempts to write all `buf.len()` bytes from `buf` into the stream. Returns `self` and `buf`
    /// once all of the bytes have been written.
    fn write<T>(self, buf: T) -> Promise<(Self, T), Error<(Self, T)>>
        where T: AsRef<[u8]>, Self: Sized;
}

pub struct Slice<T> where T: AsRef<[u8]> {
    pub buf: T,
    pub end: usize,
}

impl <T> Slice<T> where T: AsRef<[u8]> {
    pub fn new(buf: T, end: usize) -> Slice<T> {
        Slice { buf: buf, end: end }
    }
}

impl <T> AsRef<[u8]> for Slice<T> where T: AsRef<[u8]> {
    fn as_ref<'a>(&'a self) -> &'a [u8] {
        &self.buf.as_ref()[0..self.end]
    }
}

fn register_new_handle<E>(evented: &E) -> Result<Handle, ::std::io::Error> where E: ::mio::Evented {
    let handle = FdObserver::new();
    let token = ::mio::Token(handle.val);
    return with_current_event_loop(move |event_loop| {
        try!(event_loop.event_port.borrow_mut().reactor.register(evented, token,
                                                                 ::mio::EventSet::writable() |
                                                                 ::mio::EventSet::readable(),
                                                                 ::mio::PollOpt::edge()));
        // XXX if this fails, the handle does not get cleaned up.
        return Ok(handle);
    });
}

trait HasHandle {
    fn get_handle(&self) -> Handle;
}

fn try_read_internal<R, T>(mut reader: R,
                           mut buf: T,
                           mut already_read: usize,
                           min_bytes: usize) -> Promise<(R, T, usize), Error<(R, T)>>
    where T: AsMut<[u8]>, R: ::std::io::Read + HasHandle
{
    use std::io::Read;

    while already_read < min_bytes {
        match reader.read(&mut buf.as_mut()[already_read..]) {
            Ok(0) => {
                // EOF
                return Promise::ok((reader, buf, already_read));
            }
            Ok(n) => {
                already_read += n;
            }
            Err(e) => {
                if e.kind() != ::std::io::ErrorKind::WouldBlock {
                    return Promise::err(Error::new((reader, buf), e))
                } else {
                    return with_current_event_loop(move |event_loop| {
                        let promise =
                            event_loop.event_port.borrow_mut()
                            .handler.observers[reader.get_handle()].when_becomes_readable();
                        promise.then_else(move |r| match r {
                            Ok(()) => try_read_internal(reader, buf, already_read, min_bytes),
                            Err(e) => Promise::err(Error::new((reader, buf), e))
                        })
                    });
                }
            }
        }
    }

    Promise::ok((reader, buf, already_read))
}

fn write_internal<W, T>(mut writer: W,
                        buf: T,
                        mut already_written: usize) -> Promise<(W, T), Error<(W, T)>>
    where T: AsRef<[u8]>, W: ::std::io::Write + HasHandle
{
    use ::std::io::Write;

    while already_written < buf.as_ref().len() {
        match writer.write(&buf.as_ref()[already_written..]) {
            Ok(n) => {
                already_written += n;
            }
            Err(e) => {
                if e.kind() != ::std::io::ErrorKind::WouldBlock {
                    return Promise::err(Error::new((writer, buf), e))
                } else {
                    return with_current_event_loop(move |event_loop| {
                        let promise =
                            event_loop.event_port.borrow_mut()
                            .handler.observers[writer.get_handle()].when_becomes_writable();
                        promise.then_else(move |r| match r {
                            Ok(()) => write_internal(writer, buf, already_written),
                            Err(e) => Promise::err(Error::new((writer, buf), e))
                        })
                    });
                }
            }
        }
    }

    Promise::ok((writer, buf))
}

struct FdObserver {
    read_fulfiller: Option<PromiseFulfiller<(), ::std::io::Error>>,
    write_fulfiller: Option<PromiseFulfiller<(), ::std::io::Error>>,
}

impl FdObserver {
    pub fn new() -> Handle {
        with_current_event_loop(move |event_loop| {

            let observer = FdObserver { read_fulfiller: None, write_fulfiller: None };
            let event_port = &mut *event_loop.event_port.borrow_mut();
            return event_port.handler.observers.push(observer);
        })
    }

    pub fn when_becomes_readable(&mut self) -> Promise<(), ::std::io::Error> {
        let (promise, fulfiller) = new_promise_and_fulfiller();
        self.read_fulfiller = Some(fulfiller);
        return promise;
    }

    pub fn when_becomes_writable(&mut self) -> Promise<(), ::std::io::Error> {
        let (promise, fulfiller) = new_promise_and_fulfiller();
        self.write_fulfiller = Some(fulfiller);
        return promise;
    }
}

pub struct MioEventPort {
    handler: Handler,
    reactor: ::mio::EventLoop<Handler>,
}

struct Handler {
    observers: HandleTable<FdObserver>,
}

impl MioEventPort {
    pub fn new() -> Result<MioEventPort, ::std::io::Error> {
        Ok(MioEventPort {
            handler: Handler { observers: HandleTable::new() },
            reactor: try!(::mio::EventLoop::new()),
        })
    }
}

impl ::mio::Handler for Handler {
    type Timeout = Timeout;
    type Message = ();
    fn ready(&mut self, _event_loop: &mut ::mio::EventLoop<Handler>,
             token: ::mio::Token, events: ::mio::EventSet) {
        if events.is_readable() {
            match ::std::mem::replace(&mut self.observers[Handle {val: token.0}].read_fulfiller, None) {
                Some(fulfiller) => {
                    fulfiller.fulfill(())
                }
                None => {
                    ()
                }
            }
        }
        if events.is_writable() {
            match ::std::mem::replace(&mut self.observers[Handle { val: token.0}].write_fulfiller, None) {
                Some(fulfiller) => fulfiller.fulfill(()),
                None => (),
            }
        }
    }
    fn timeout(&mut self, _event_loop: &mut ::mio::EventLoop<Handler>, timeout: Timeout) {
        timeout.fulfiller.fulfill(());
    }
}

impl EventPort for MioEventPort {
    fn wait(&mut self) -> bool {
        self.reactor.run_once(&mut self.handler, None).unwrap();
        false
    }

    fn poll(&mut self) -> bool {
        self.reactor.run_once(&mut self.handler, None).unwrap();
        false
    }
}

pub struct Timer;

impl Timer {
    pub fn after_delay_ms(&self, delay: u64) -> Promise<(), ::std::io::Error> {
        let (promise, fulfiller) = new_promise_and_fulfiller();
        let timeout = Timeout { fulfiller: fulfiller };
        with_current_event_loop(move |event_loop| {
            let handle = match event_loop.event_port.borrow_mut().reactor.timeout_ms(timeout, delay) {
                Ok(v) => v,
                Err(_) => return Promise::err(::std::io::Error::new(::std::io::ErrorKind::Other,
                                                                    "mio timer error"))
            };
            Promise {
                node: Box::new(
                    ::private::promise_node::Wrapper::new(promise.node,
                                                          TimeoutDropper { handle: handle })) }
        })
    }

    pub fn timeout_after_ms<T>(&self, delay: u64,
                               promise: Promise<T, ::std::io::Error>) -> Promise<T, ::std::io::Error>
    {
        promise.exclusive_join(self.after_delay_ms(delay).map(|()| {
            Err(::std::io::Error::new(::std::io::ErrorKind::Other, "operation timed out"))
        }))
    }
}

struct TimeoutDropper {
    handle: ::mio::Timeout,
}

impl Drop for TimeoutDropper {
    fn drop(&mut self) {
        with_current_event_loop(move |event_loop| {
            event_loop.event_port.borrow_mut().reactor.clear_timeout(self.handle);
        });
    }
}

struct Timeout {
    fulfiller: PromiseFulfiller<(), ::std::io::Error>,
}
