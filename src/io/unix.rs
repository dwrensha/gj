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

use std::ops::{DerefMut, Deref};
use handle_table::{Handle};
use ::io::{AsyncRead, AsyncWrite, try_read_internal, write_internal,
           FdObserver, HasHandle, register_new_handle};
use {EventLoop, Promise, Result, WaitScope};
use private::{with_current_event_loop};

pub struct UnixStream {
    stream: ::mio::unix::UnixStream,
    handle: Handle,
}

pub struct UnixListener {
    listener: ::mio::unix::UnixListener,
    handle: Handle,
}

impl Drop for UnixListener {
    fn drop(&mut self) {
        // deregister the token
    }
}

impl UnixListener {
    pub fn bind<P: AsRef<::std::path::Path>>(addr: P) -> Result<UnixListener> {
        let listener = try!(::mio::unix::UnixListener::bind(&addr));
        let handle = FdObserver::new();

        return with_current_event_loop(move |event_loop| {
            try!(event_loop.event_port.borrow_mut().reactor.register_opt(&listener, ::mio::Token(handle.val),
                                                                         ::mio::EventSet::readable(),
                                                                         ::mio::PollOpt::edge()));
            Ok(UnixListener { listener: listener,
                              handle: handle })
        });
    }

    fn accept_internal(self) -> Result<Promise<(UnixListener, UnixStream)>> {
        let accept_result = try!(self.listener.accept());
        match accept_result {
            Some(stream) => {
                let handle = try!(register_new_handle(&stream));
                return Ok(Promise::fulfilled((self, UnixStream::new(stream, handle))));
            }
            None => {
                return with_current_event_loop(move |event_loop| {
                    let promise =
                        event_loop.event_port.borrow_mut().handler.observers[self.handle].when_becomes_readable();
                    return Ok(promise.then(move |()| {
                        return self.accept_internal();
                    }));
                });
            }
        }
    }

    pub fn accept(self) -> Promise<(UnixListener, UnixStream)> {
        return Promise::fulfilled(()).then(move |()| {return self.accept_internal(); });
    }
}

impl ::mio::TryRead for UnixStream {
    fn try_read(&mut self, buf: &mut [u8]) -> ::std::io::Result<Option<usize>> {
        use mio::TryRead;
        self.stream.try_read(buf)
    }
}

impl ::mio::TryWrite for UnixStream {
    fn try_write(&mut self, buf: &[u8]) -> ::std::io::Result<Option<usize>> {
        use mio::TryWrite;
        self.stream.try_write(buf)
    }
}

impl HasHandle for UnixStream {
    fn get_handle(&self) -> Handle { self.handle }
}

impl UnixStream {
    fn new(stream: ::mio::unix::UnixStream, handle: Handle) -> UnixStream {
        UnixStream { stream: stream, handle: handle }
    }

    pub fn connect<P: AsRef<::std::path::Path>>(addr: P) -> Promise<UnixStream> {
        let connect_result = ::mio::unix::UnixStream::connect(&addr);
        return Promise::fulfilled(()).then(move |()| {
            let stream = try!(connect_result);

            // TODO: if we're not already connected, maybe only register writable interest,
            // and then reregister with read/write interested once we successfully connect.

            let handle = try!(register_new_handle(&stream));

            return with_current_event_loop(move |event_loop| {
                let promise =
                    event_loop.event_port.borrow_mut().handler.observers[handle].when_becomes_writable();
                return Ok(promise.map(move |()| {
                    //try!(stream.take_socket_error());
                    return Ok(UnixStream::new(stream, handle));
                }));
            });
        });
    }

    pub fn try_clone(&self) -> Result<UnixStream> {
        let stream = try!(self.stream.try_clone());
        let handle = try!(register_new_handle(&stream));
        return Ok(UnixStream::new(stream, handle));
    }
}

impl AsyncRead for UnixStream {
    fn try_read<T>(self, buf: T,
               min_bytes: usize) -> Promise<(Self, T, usize)> where T: DerefMut<Target=[u8]> {
        return Promise::fulfilled(()).then(move |()| {
            return try_read_internal(self, buf, 0, min_bytes);
        });
    }
}

impl AsyncWrite for UnixStream {
    fn write<T>(self, buf: T) -> Promise<(Self, T)> where T: Deref<Target=[u8]> {
        return Promise::fulfilled(()).then(move |()| {
            return write_internal(self, buf, 0);
        });
    }
}

/// Creates a new thread and sets up a socket pair that can be used to communicate with it.
/// Passes one of the sockets to the thread's start function and returns the other socket.
/// The new thread will already have an active event loop when `start_func` is called.
pub fn spawn<F>(start_func: F) -> Result<(::std::thread::JoinHandle<()>, UnixStream)>
    where F: FnOnce(UnixStream, &WaitScope) -> Result<()>,
          F: Send + 'static
{
    use nix::sys::socket::{socketpair, AddressFamily, SockType, SOCK_CLOEXEC, SOCK_NONBLOCK};
    use std::os::unix::io::FromRawFd;

    let (fd0, fd1) =
        // eh... the trait `std::error::Error` is not implemented for the type `nix::Error`
        match socketpair(AddressFamily::Unix, SockType::Stream, 0, SOCK_NONBLOCK | SOCK_CLOEXEC) {
            Ok(v) => v,
            Err(_) => {
                return Err(Box::new(::std::io::Error::new(::std::io::ErrorKind::Other,
                                                          "failed to create socketpair")))
            }
        };

    let io = unsafe { ::mio::unix::UnixStream::from_raw_fd(fd0) };
    let handle = try!(register_new_handle(&io));
    let socket_stream = UnixStream { stream: io, handle: handle };

    let join_handle = ::std::thread::spawn(move || {
        let _result = EventLoop::top_level(move |wait_scope| {
            let io = unsafe { ::mio::unix::UnixStream::from_raw_fd(fd1) };
            let handle = try!(register_new_handle(&io));
            let socket_stream = UnixStream { stream: io, handle: handle };
            start_func(socket_stream, &wait_scope)
        });
    });

    return Ok((join_handle, socket_stream));
}
