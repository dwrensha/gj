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

//! TCP sockets.

use std::result::Result;
use io::register_new_handle;
use Promise;
use private::with_current_event_loop;

pub type Stream = ::io::stream::Stream<::mio::tcp::TcpStream>;

pub type Listener = ::io::stream::Listener<::mio::tcp::TcpListener>;

impl Listener {
    pub fn bind(addr: ::std::net::SocketAddr) -> Result<Listener, ::std::io::Error> {
        Listener::new(try!(::mio::tcp::TcpListener::bind(&addr)))
    }
}

impl Stream {
    pub fn connect(addr: ::std::net::SocketAddr) -> Promise<Stream, ::std::io::Error> {
        Promise::ok(()).then(move |()| {
            let stream = pry!(::mio::tcp::TcpStream::connect(&addr));

            // TODO: if we're not already connected, maybe only register writable interest,
            // and then reregister with read/write interested once we successfully connect.

            let handle = pry!(register_new_handle(&stream));

            with_current_event_loop(move |event_loop| {
                let promise = event_loop.event_port.borrow_mut().handler.observers[handle]
                                  .when_becomes_writable();
                promise.map(move |()| {
                    try!(stream.take_socket_error());
                    Ok(Stream::new(stream, handle))
                })
            })
        })
    }

    pub fn try_clone(&self) -> Result<Stream, ::std::io::Error> {
        let stream = try!(self.stream().try_clone());
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

pub type Reader = ::io::stream::Reader<::mio::tcp::TcpStream>;

pub type Writer = ::io::stream::Writer<::mio::tcp::TcpStream>;
