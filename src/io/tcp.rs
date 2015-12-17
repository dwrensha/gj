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
use handle_table::{Handle};
use ::io::{AsyncRead, AsyncWrite, try_read_internal, write_internal,
           FdObserver, HasHandle, register_new_handle,
           Error};
use {Promise};
use private::{with_current_event_loop};

pub struct Stream {
    stream: ::mio::tcp::TcpStream,
    handle: Handle,
    no_send: ::std::marker::PhantomData<*mut ()>, // impl !Send for Stream
}

pub struct Listener {
    listener: ::mio::tcp::TcpListener,
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
    pub fn bind(addr: ::std::net::SocketAddr) -> Result<Listener, ::std::io::Error> {
        let listener = try!(::mio::tcp::TcpListener::bind(&addr));
        let handle = FdObserver::new();

        with_current_event_loop(move |event_loop| {
            try!(event_loop.event_port.borrow_mut().reactor.register(&listener, ::mio::Token(handle.val),
                                                                     ::mio::EventSet::readable(),
                                                                     ::mio::PollOpt::edge()));
            Ok(Listener { listener: listener, handle: handle, no_send: ::std::marker::PhantomData })
        })
    }

    fn accept_internal(self) -> Result<Promise<(Listener, Stream), Error<Listener>>, Error<Listener>> {
        let accept_result = match self.listener.accept() {
            Err(e) => return Err(Error::new(self, e)),
            Ok(v) => v,
        };
        match accept_result {
            Some((stream, _)) => {
                let handle = match register_new_handle(&stream) {
                    Err(e) => return Err(Error::new(self, e)),
                    Ok(v) => v,
                };
                Ok(Promise::fulfilled((self, Stream::new(stream, handle))))
            }
            None => {
                with_current_event_loop(move |event_loop| {
                    let promise =
                        event_loop.event_port.borrow_mut().handler.observers[self.handle].when_becomes_readable();
                    Ok(promise.then_else(move |r| match r {
                        Ok(()) => self.accept_internal(),
                        Err(e) => Err(Error::new(self, e))
                    }))
                })
            }
        }
    }

    pub fn accept(self) -> Promise<(Listener, Stream), Error<Listener>> {
        Promise::fulfilled(()).then(move |()| self.accept_internal())
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
    fn new(stream: ::mio::tcp::TcpStream, handle: Handle) -> Stream {
        Stream { stream: stream, handle: handle, no_send: ::std::marker::PhantomData }
    }

    pub fn connect(addr: ::std::net::SocketAddr) -> Promise<Stream, ::std::io::Error> {
        Promise::fulfilled(()).then(move |()| {
            let stream = try!(::mio::tcp::TcpStream::connect(&addr));

            // TODO: if we're not already connected, maybe only register writable interest,
            // and then reregister with read/write interested once we successfully connect.

            let handle = try!(register_new_handle(&stream));

            with_current_event_loop(move |event_loop| {
                let promise =
                    event_loop.event_port.borrow_mut().handler.observers[handle].when_becomes_writable();
                Ok(promise.map(move |()| {
                    try!(stream.take_socket_error());
                    Ok(Stream::new(stream, handle))
                }))
            })
        })
    }

    pub fn try_clone(&self) -> Result<Stream, ::std::io::Error> {
        let stream = try!(self.stream.try_clone());
        let handle = try!(register_new_handle(&stream));
        Ok(Stream::new(stream, handle))
    }

    #[cfg(unix)]
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
        Promise::fulfilled(()).then(move |()| {
            try_read_internal(self, buf, 0, min_bytes)
        })
    }
}

impl AsyncWrite for Stream {
    fn write<T>(self, buf: T) -> Promise<(Self, T), Error<(Self, T)>> where T: AsRef<[u8]> {
        Promise::fulfilled(()).then(move |()| {
            write_internal(self, buf, 0)
        })
    }
}
