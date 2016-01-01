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

use std::result::Result;
use std::rc::Rc;
use std::cell::RefCell;
use handle_table::{Handle};
use ::io::{AsyncRead, AsyncWrite, try_read_internal, write_internal,
           FdObserver, HasHandle, register_new_handle,
           Error};
use {EventLoop, Promise, WaitScope};
use private::{with_current_event_loop};

pub struct Stream {
    stream: ::mio::unix::UnixStream,
    handle: Handle,
    no_send: ::std::marker::PhantomData<*mut ()>, // impl !Send for Stream
}

pub struct Listener {
    listener: ::mio::unix::UnixListener,
    handle: Handle,
    no_send: ::std::marker::PhantomData<*mut ()>, // impl !Send for Listener
}

impl Drop for Listener {
    fn drop(&mut self) {
        with_current_event_loop(move |event_loop| {
            event_loop.event_port.borrow_mut().handler.observers.remove(self.handle);
        })
    }
}

impl Listener {
    pub fn bind<P: AsRef<::std::path::Path>>(addr: P) -> Result<Listener, ::std::io::Error> {
        let listener = try!(::mio::unix::UnixListener::bind(&addr));
        let handle = FdObserver::new();

        with_current_event_loop(move |event_loop| {
            try!(event_loop.event_port.borrow_mut().reactor.register(&listener, ::mio::Token(handle.val),
                                                                         ::mio::EventSet::readable(),
                                                                         ::mio::PollOpt::edge()));
            Ok(Listener { listener: listener, handle: handle, no_send: ::std::marker::PhantomData })
        })
    }

    fn accept_internal(self) -> Promise<(Listener, Stream), Error<Listener>> {
        let accept_result = match self.listener.accept() {
            Err(e) => return Promise::err((Error::new(self, e))),
            Ok(v) => v,
        };
        match accept_result {
            Some(stream) => {
                let handle = match register_new_handle(&stream) {
                    Err(e) => return Promise::err(Error::new(self, e)),
                    Ok(v) => v,
                };
                Promise::ok((self, Stream::new(stream, handle)))
            }
            None => {
                with_current_event_loop(move |event_loop| {
                    let promise =
                        event_loop.event_port.borrow_mut().handler.observers[self.handle].when_becomes_readable();
                    promise.then_else(move |r| match r {
                        Ok(()) => self.accept_internal(),
                        Err(e) => Promise::err(Error::new(self, e))
                    })
                })
            }
        }
    }

    pub fn accept(self) -> Promise<(Listener, Stream), Error<Listener>> {
        Promise::ok(()).then(move |()| self.accept_internal())
    }
}

impl ::std::io::Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> ::std::io::Result<usize> {
        use std::io::Read;
        self.stream.read(buf)
    }
}

impl ::std::io::Write for Stream {
    fn write(&mut self, buf: &[u8]) -> ::std::io::Result<usize> {
        use std::io::Write;
        self.stream.write(buf)
    }
    fn flush(&mut self) -> ::std::io::Result<()> { Ok(()) }
}

impl HasHandle for Stream {
    fn get_handle(&self) -> Handle { self.handle }
}

impl Drop for Stream {
    fn drop(&mut self) {
        with_current_event_loop(move |event_loop| {
            event_loop.event_port.borrow_mut().handler.observers.remove(self.handle);
        })
    }
}

impl Stream {
    fn new(stream: ::mio::unix::UnixStream, handle: Handle) -> Stream {
        Stream { stream: stream, handle: handle, no_send: ::std::marker::PhantomData }
    }

    pub fn connect<P: AsRef<::std::path::Path>>(addr: P) -> Promise<Stream, ::std::io::Error> {
        let connect_result = ::mio::unix::UnixStream::connect(&addr);
        Promise::ok(()).then(move |()| {
            let stream = pry!(connect_result);

            // TODO: if we're not already connected, maybe only register writable interest,
            // and then reregister with read/write interested once we successfully connect.

            let handle = pry!(register_new_handle(&stream));

            with_current_event_loop(move |event_loop| {
                let promise =
                    event_loop.event_port.borrow_mut().handler.observers[handle].when_becomes_writable();
                promise.map(move |()| {
                    //try!(stream.take_socket_error());
                    Ok(Stream::new(stream, handle))
                })
            })
        })
    }

    pub fn new_pair() -> Result<(Stream, Stream), ::std::io::Error> {
        use nix::sys::socket::{socketpair, AddressFamily, SockType, SOCK_CLOEXEC, SOCK_NONBLOCK};
        use std::os::unix::io::FromRawFd;

        let (fd1, fd2) =
            try!(socketpair(AddressFamily::Unix, SockType::Stream, 0, SOCK_NONBLOCK | SOCK_CLOEXEC)
                 .map_err(|_| ::std::io::Error::new(::std::io::ErrorKind::Other,
                                                    "failed to create socketpair")));

        unsafe { Ok((try!(Stream::from_raw_fd(fd1)), try!(Stream::from_raw_fd(fd2)))) }
    }

    pub fn try_clone(&self) -> Result<Stream, ::std::io::Error> {
        let stream = try!(self.stream.try_clone());
        let handle = try!(register_new_handle(&stream));
        Ok(Stream::new(stream, handle))
    }

    pub fn split(self) -> (Reader, Writer) {
        let inner = Rc::new(RefCell::new(self));
        (Reader { stream: inner.clone() }, Writer { stream: inner })
    }

    pub unsafe fn from_raw_fd(fd: ::std::os::unix::io::RawFd) -> Result<Stream, ::std::io::Error> {
        let stream = ::std::os::unix::io::FromRawFd::from_raw_fd(fd);
        let handle = try!(register_new_handle(&stream));
        Ok(Stream::new(stream, handle))
    }
}

impl AsyncRead for Stream {
    fn try_read<T>(self, buf: T,
                   min_bytes: usize) -> Promise<(Self, T, usize), Error<(Self, T)>>
        where T: AsMut<[u8]>
    {
        try_read_internal(self, buf, 0, min_bytes)
    }
}

impl AsyncWrite for Stream {
    fn write<T>(self, buf: T) -> Promise<(Self, T), Error<(Self, T)>> where T: AsRef<[u8]> {
        write_internal(self, buf, 0)
    }
}

/// Creates a new thread and sets up a socket pair that can be used to communicate with it.
/// Passes one of the sockets to the thread's start function and returns the other socket.
/// The new thread will already have an active event loop when `start_func` is called.
pub fn spawn<F>(start_func: F) -> Result<(::std::thread::JoinHandle<()>, Stream), Box<::std::error::Error>>
    where F: FnOnce(Stream, &WaitScope) -> Result<(), Box<::std::error::Error>>,
          F: Send + 'static
{
    use nix::sys::socket::{socketpair, AddressFamily, SockType, SOCK_CLOEXEC, SOCK_NONBLOCK};
    use std::os::unix::io::FromRawFd;

    let (fd0, fd1) =
        try!(socketpair(AddressFamily::Unix, SockType::Stream, 0, SOCK_NONBLOCK | SOCK_CLOEXEC));

    let socket_stream = try!(unsafe { Stream::from_raw_fd(fd0) });

    let join_handle = ::std::thread::spawn(move || {
        let _result = EventLoop::top_level(move |wait_scope| {
            let socket_stream = try!(unsafe { Stream::from_raw_fd(fd1) });
            start_func(socket_stream, &wait_scope)
        });
    });

    return Ok((join_handle, socket_stream));
}

pub struct Reader {
    stream: Rc<RefCell<Stream>>
}

impl ::std::io::Read for Reader {
    fn read(&mut self, buf: &mut [u8]) -> ::std::io::Result<usize> {
        use std::io::Read;
        self.stream.borrow_mut().stream.read(buf)
    }
}

impl HasHandle for Reader {
    fn get_handle(&self) -> Handle { self.stream.borrow().handle }
}

impl AsyncRead for Reader {
    fn try_read<T>(self, buf: T, min_bytes: usize) -> Promise<(Self, T, usize), Error<(Self, T)>>
        where T: AsMut<[u8]>
    {
        try_read_internal(self, buf, 0, min_bytes)
    }
}

pub struct Writer {
    stream: Rc<RefCell<Stream>>
}

impl ::std::io::Write for Writer {
    fn write(&mut self, buf: &[u8]) -> ::std::io::Result<usize> {
        use std::io::Write;
        self.stream.borrow_mut().stream.write(buf)
    }
    fn flush(&mut self) -> ::std::io::Result<()> { Ok(()) }
}

impl HasHandle for Writer {
    fn get_handle(&self) -> Handle { self.stream.borrow().handle }
}

impl AsyncWrite for Writer {
    fn write<T>(self, buf: T) -> Promise<(Self, T), Error<(Self, T)>> where T: AsRef<[u8]> {
        write_internal(self, buf, 0)
    }
}
