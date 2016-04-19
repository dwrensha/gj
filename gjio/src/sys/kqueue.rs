// Copyright (c) 2013-2016 Sandstorm Development Group, Inc. and contributors
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

use std::os::unix::io::RawFd;
use gj::{Promise, PromiseFulfiller};
use handle_table::{HandleTable, Handle};
use nix::sys::event;

pub struct FdObserver {
    read_fulfiller: Option<PromiseFulfiller<(), ::std::io::Error>>,
    write_fulfiller: Option<PromiseFulfiller<(), ::std::io::Error>>,
}

impl FdObserver {
    pub fn when_becomes_readable(&mut self) -> Promise<(), ::std::io::Error> {
        let (promise, fulfiller) = Promise::and_fulfiller();
        self.read_fulfiller = Some(fulfiller);
        promise
    }

    pub fn when_becomes_writable(&mut self) -> Promise<(), ::std::io::Error> {
        let (promise, fulfiller) = Promise::and_fulfiller();
        self.write_fulfiller = Some(fulfiller);
        promise
    }
}

pub struct Reactor {
    pub kq: RawFd,
    pub observers: HandleTable<FdObserver>,
    events: Vec<event::KEvent>,
}

impl Reactor {
    pub fn new() -> Result<Reactor, ::std::io::Error> {
        Ok(Reactor {
            kq: try!(event::kqueue()),
            observers: HandleTable::new(),
            events: Vec::with_capacity(1024),
        })
    }

    pub fn run_once(&mut self) -> Result<(), ::std::io::Error> {

        let events = unsafe {
            let ptr = (&mut self.events[..]).as_mut_ptr();
            ::std::slice::from_raw_parts_mut(ptr, self.events.capacity())
        };

       // TODO handle EINTR
        let n = try!(event::kevent(self.kq, &[], events, 0));

        for event in &events[..n] {
            let handle = Handle { val: event.udata };

            let maybe_fulfiller = match event.filter {
                event::EventFilter::EVFILT_READ => self.observers[handle].read_fulfiller.take(),
                event::EventFilter::EVFILT_WRITE => self.observers[handle].write_fulfiller.take(),
                _ => unreachable!()
            };

            match maybe_fulfiller {
                None => (),
                Some(fulfiller) => {
                    if event.flags.contains(event::EV_ERROR) {
                        fulfiller.reject(::std::io::Error::from_raw_os_error(event.data as i32));
                    } else {
                        fulfiller.fulfill(());
                    }
                }
            }
        }

        Ok(())
    }


    pub fn new_observer(&mut self, fd: RawFd) -> Result<Handle, ::std::io::Error> {
        let observer = FdObserver { read_fulfiller: None, write_fulfiller: None };
        let handle = self.observers.push(observer);

        let read_event = event::KEvent {
            ident: fd as usize,
            filter: event::EventFilter::EVFILT_READ,
            flags: event::EV_ADD | event::EV_CLEAR,
            fflags: event::FilterFlag::empty(),
            data: 0,
            udata: handle.val
        };

        let write_event = event::KEvent {
            ident: fd as usize,
            filter: event::EventFilter::EVFILT_WRITE,
            flags: event::EV_ADD | event::EV_CLEAR,
            fflags: event::FilterFlag::empty(),
            data: 0,
            udata: handle.val
        };


        // TODO handle EINTR?
        try!(event::kevent(self.kq, &[read_event, write_event], &mut[], 0));

        Ok(handle)
    }
}
/*
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

struct Timeout {
    fulfiller: PromiseFulfiller<(), ::std::io::Error>,
}
*/
