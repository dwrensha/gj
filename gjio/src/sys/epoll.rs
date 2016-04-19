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
use handle_table::{HandleTable, Handle};
use sys::FdObserver;
use nix::sys::epoll;

pub struct Reactor {
    pub ep: RawFd,
    pub observers: HandleTable<FdObserver>,
    events: Vec<epoll::EpollEvent>
}

impl Reactor {
    pub fn new() -> Result<Reactor, ::std::io::Error> {
        Ok(Reactor {
            ep: try_syscall!(epoll::epoll_create()),
            observers: HandleTable::new(),
            events: Vec::with_capacity(1024),
        })
    }

    pub fn run_once(&mut self) -> Result<(), ::std::io::Error> {

        let events = unsafe {
            let ptr = (&mut self.events[..]).as_mut_ptr();
            ::std::slice::from_raw_parts_mut(ptr, self.events.capacity())
        };

        let n = try_syscall!(epoll::epoll_wait(self.ep, events, -1));

        unsafe { self.events.set_len(n); }

        for event in &self.events {
            let handle = Handle { val: event.data as usize };

            if event.events.contains(epoll::EPOLLIN) || event.events.contains(epoll::EPOLLHUP) ||
               event.events.contains(epoll::EPOLLERR)
            {
                match self.observers[handle].read_fulfiller.take() {
                    None => (),
                    Some(fulfiller) => {
                        fulfiller.fulfill(());
                    }
                }
            }

            if event.events.contains(epoll::EPOLLOUT) || event.events.contains(epoll::EPOLLHUP) ||
               event.events.contains(epoll::EPOLLERR)
            {
                match self.observers[handle].write_fulfiller.take() {
                    None => (),
                    Some(fulfiller) => {
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

        let info = epoll::EpollEvent {
            events: epoll::EPOLLIN | epoll::EPOLLOUT,
            data: handle.val as u64
        };

        try_syscall!(epoll::epoll_ctl(self.ep, epoll::EpollOp::EpollCtlAdd, fd, &info));

        Ok(handle)
    }
}
