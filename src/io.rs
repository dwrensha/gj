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
use {EventPort, Promise, Result};

pub trait AsyncRead {
    fn read(self: Box<Self>, buf: Vec<u8>, min_bytes: usize, max_bytes: usize)
            -> Promise<(Box<Self>, Vec<u8>, usize)>;
}


pub trait AsyncWrite {

    // Hm. Seems like the caller is often going to need to do an extra copy here.
    // Can we avoid that somehow?
    fn write(self: Box<Self>, buf: Vec<u8>) -> Promise<(Box<Self>, Vec<u8>)>;
}



/*
pub struct AsyncIoContext {
    x: (),
}
*/


pub struct FdObserver {
    _fd: ::std::os::unix::io::RawFd,
//    read_fulfiller: Option<>,
}

pub struct EventPortImpl {
    handler: Handler,
    reactor: ::mio::EventLoop<Handler>,
}

pub struct Handler {
    observers: Slab<FdObserver>,
}

impl EventPortImpl {
    pub fn new() -> Result<EventPortImpl> {
        Ok(EventPortImpl {
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
        &self.observers[token];
    }
    fn writable(&mut self, _event_loop: &mut ::mio::EventLoop<Handler>, _token: ::mio::Token) {

    }
}

impl EventPort for EventPortImpl {
    fn wait(&mut self) -> bool {
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
