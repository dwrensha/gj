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

use mio::util::Slab;
use {EventPort, Promise, PromiseFulfiller, Result, new_promise_and_fulfiller};
use private::{with_current_event_loop};

pub trait AsyncRead {
    fn read(self: Box<Self>, buf: Vec<u8>, min_bytes: usize, max_bytes: usize)
            -> Promise<(Box<Self>, Vec<u8>, usize)>;
}


pub trait AsyncWrite {

    // Hm. Seems like the caller is often going to need to do an extra copy here.
    // Can we avoid that somehow?
    fn write(self: Box<Self>, buf: Vec<u8>) -> Promise<(Box<Self>, Vec<u8>)>;
}


#[derive(Copy, Clone)]
pub struct NetworkAddress {
    address: ::std::net::SocketAddr,
}

impl NetworkAddress {
    pub fn new(address: ::std::net::SocketAddr) -> NetworkAddress {
        NetworkAddress { address: address }
    }

    pub fn listen(self) -> Result<ConnectionReceiver> {
        let socket = try!(::mio::tcp::TcpSocket::v4());
        try!(socket.bind(&self.address));
        let token = FdObserver::new();
        Ok(ConnectionReceiver { listener: try!(socket.listen(256)),
                                token: token })
    }

    pub fn connect(self) -> Promise<AsyncIoStream> {
        let socket = ::mio::tcp::TcpSocket::v4().unwrap();
        let token = FdObserver::new();
        let (stream, _) = socket.connect(&self.address).unwrap();
        with_current_event_loop(move |event_loop| {
            let promise =
                event_loop.event_port.borrow_mut().handler.observers[token].when_becomes_writable();
            return promise.map(move |()| {
                return Ok(AsyncIoStream { stream: stream,
                                          token: token });
            });
        })
    }
}


pub struct ConnectionReceiver {
    listener: ::mio::tcp::TcpListener,
    token: ::mio::Token,
}

impl Drop for ConnectionReceiver {
    fn drop(&mut self) {
        // deregister the token
    }
}

impl ConnectionReceiver {
    pub fn accept(self) -> Promise<(ConnectionReceiver, AsyncIoStream)> {
        with_current_event_loop(move |event_loop| {
            let promise =
                event_loop.event_port.borrow_mut().handler.observers[self.token].when_becomes_readable();
            event_loop.event_port.borrow_mut().reactor.register_opt(&self.listener, self.token,
                                                                    ::mio::Interest::readable(),
                                                                    ::mio::PollOpt::edge()).unwrap();
            return promise.map(move |()| {
                let stream = self.listener.accept().unwrap().unwrap();
                let token = FdObserver::new();
                return Ok((self, AsyncIoStream { stream: stream, token: token }));
            });
        })
    }
}

#[allow(dead_code)]
pub struct AsyncIoStream {
    stream: ::mio::tcp::TcpStream,
    token: ::mio::Token,
}

/*
pub struct AsyncIoContext {
    x: (),
}
*/


pub struct FdObserver {
    read_fulfiller: Option<Box<PromiseFulfiller<()>>>,
    write_fulfiller: Option<Box<PromiseFulfiller<()>>>,
}

impl FdObserver {
    pub fn new() -> ::mio::Token {
        with_current_event_loop(move |event_loop| {

            let observer = FdObserver { read_fulfiller: None, write_fulfiller: None };
            let event_port = &mut *event_loop.event_port.borrow_mut();
            let token = match event_port.handler.observers.insert(observer) {
                Ok(tok) => tok,
                Err(_obs) => panic!("could not insert observer."),
            };
            return token;
        })
    }

    pub fn when_becomes_readable(&mut self) -> Promise<()> {
        let (promise, fulfiller) = new_promise_and_fulfiller();
        self.read_fulfiller = Some(fulfiller);
        return promise;
    }

    pub fn when_becomes_writable(&mut self) -> Promise<()> {
        let (promise, fulfiller) = new_promise_and_fulfiller();
        self.write_fulfiller = Some(fulfiller);
        return promise;
    }
}

pub struct MioEventPort {
    handler: Handler,
    reactor: ::mio::EventLoop<Handler>,
}

pub struct Handler {
    observers: Slab<FdObserver>,
}

impl MioEventPort {
    pub fn new() -> Result<MioEventPort> {
        Ok(MioEventPort {
            handler: Handler { observers: Slab::new(100) },
            reactor: try!(::mio::EventLoop::new()),
        })
    }
}

impl ::mio::Handler for Handler {
    type Timeout = ();
    type Message = ();
    fn readable(&mut self, _event_loop: &mut ::mio::EventLoop<Handler>,
                token: ::mio::Token, _hint: ::mio::ReadHint) {
        match ::std::mem::replace(&mut self.observers[token].read_fulfiller, None) {
            Some(fulfiller) => {
                fulfiller.fulfill(())
            }
            None => {
                ()
            }
        }
    }
    fn writable(&mut self, _event_loop: &mut ::mio::EventLoop<Handler>, token: ::mio::Token) {
        match ::std::mem::replace(&mut self.observers[token].write_fulfiller, None) {
            Some(fulfiller) => fulfiller.fulfill(()),
            None => (),
        }
    }
}

impl EventPort for MioEventPort {
    fn wait(&mut self) -> bool {
        self.reactor.run_once(&mut self.handler).unwrap();
        return false;
    }

    fn poll(&mut self) -> bool {
        self.reactor.run_once(&mut self.handler).unwrap();
        return false;
    }
}

// alternate approach below =========

pub enum StepResult<S, T> {
   TheresMore(S),
   Done(T)
}

/// Intermediate state for a reading a T.
pub trait AsyncReadState<T> {

   /// Reads as much as possible without blocking. If done, returns the final T value. Otherwise
   /// returns the new intermediate state T.
   fn read_step(self, r: &mut ::std::io::Read) -> ::std::io::Result<StepResult<Self, T>>;
}

/// Gives back `r` once the T has been completely read.
pub fn read_async<R, S, T>(_r: R, _state: S) -> Promise<(R, T)>
  where R: ::std::io::Read + 'static,
        S: AsyncReadState<T> + 'static,
        T: 'static {
            unimplemented!();
}
