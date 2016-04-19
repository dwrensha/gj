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

#[cfg(target_os = "macos")]
pub mod kqueue;

#[cfg(target_os = "linux")]
pub mod epoll;

#[cfg(target_os = "macos")]
pub type Reactor = kqueue::Reactor;

#[cfg(target_os = "linux")]
pub type Reactor = epoll::Reactor;

pub struct FdObserver {
    fd: RawFd,
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
