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

use std::rc::Rc;
use std::cell::RefCell;
use handle_table::Handle;
use io::{AsyncRead, AsyncWrite, try_read_internal, write_internal, HasHandle, Error};
use Promise;
use private::with_current_event_loop;

pub struct Stream<S: ::mio::Evented> {
    stream: S,
    handle: Handle,
    no_send: ::std::marker::PhantomData<*mut ()>, // impl !Send for Stream
}

impl<S> ::std::io::Read for Stream<S> where S: ::std::io::Read + ::mio::Evented
{
    fn read(&mut self, buf: &mut [u8]) -> ::std::io::Result<usize> {
        use std::io::Read;
        self.stream.read(buf)
    }
}

impl<S> ::std::io::Write for Stream<S> where S: ::std::io::Write + ::mio::Evented
{
    fn write(&mut self, buf: &[u8]) -> ::std::io::Result<usize> {
        use std::io::Write;
        self.stream.write(buf)
    }
    fn flush(&mut self) -> ::std::io::Result<()> {
        use std::io::Write;
        self.stream.flush()
    }
}

impl<S> HasHandle for Stream<S> where S: ::mio::Evented
{
    fn get_handle(&self) -> Handle {
        self.handle
    }
}

impl<S> Drop for Stream<S> where S: ::mio::Evented
{
    fn drop(&mut self) {
        with_current_event_loop(move |event_loop| {
            event_loop.event_port.borrow_mut().handler.observers.remove(self.handle);
            let _ = event_loop.event_port.borrow_mut().reactor.deregister(&self.stream);
        })
    }
}

impl<S> Stream<S> where S: ::mio::Evented
{
    pub fn new(stream: S, handle: Handle) -> Stream<S> {
        Stream {
            stream: stream,
            handle: handle,
            no_send: ::std::marker::PhantomData,
        }
    }

    pub fn split(self) -> (Reader<S>, Writer<S>) {
        let inner = Rc::new(RefCell::new(self));
        (Reader { stream: inner.clone() }, Writer { stream: inner })
    }

    pub fn stream(&self) -> &S {
        &self.stream
    }

    pub fn stream_mut(&mut self) -> &mut S {
        &mut self.stream
    }
}

impl<S> AsyncRead for Stream<S> where S: ::std::io::Read + ::mio::Evented + 'static
{
    fn try_read<T>(self, buf: T, min_bytes: usize) -> Promise<(Self, T, usize), Error<(Self, T)>>
        where T: AsMut<[u8]>
    {
        try_read_internal(self, buf, 0, min_bytes)
    }
}

impl<S> AsyncWrite for Stream<S> where S: ::std::io::Write + ::mio::Evented + 'static
{
    fn write<T>(self, buf: T) -> Promise<(Self, T), Error<(Self, T)>>
        where T: AsRef<[u8]>
    {
        write_internal(self, buf, 0)
    }
}

pub struct Reader<S: ::mio::Evented> {
    stream: Rc<RefCell<Stream<S>>>,
}

impl<S> ::std::io::Read for Reader<S> where S: ::std::io::Read + ::mio::Evented
{
    fn read(&mut self, buf: &mut [u8]) -> ::std::io::Result<usize> {
        use std::io::Read;
        self.stream.borrow_mut().stream.read(buf)
    }
}

impl<S> HasHandle for Reader<S> where S: ::mio::Evented
{
    fn get_handle(&self) -> Handle {
        self.stream.borrow().handle
    }
}

impl<S> AsyncRead for Reader<S> where S: ::std::io::Read + ::mio::Evented + 'static
{
    fn try_read<T>(self, buf: T, min_bytes: usize) -> Promise<(Self, T, usize), Error<(Self, T)>>
        where T: AsMut<[u8]>
    {
        try_read_internal(self, buf, 0, min_bytes)
    }
}

pub struct Writer<S: ::mio::Evented> {
    stream: Rc<RefCell<Stream<S>>>,
}

impl<S> ::std::io::Write for Writer<S> where S: ::std::io::Write + ::mio::Evented
{
    fn write(&mut self, buf: &[u8]) -> ::std::io::Result<usize> {
        use std::io::Write;
        self.stream.borrow_mut().stream.write(buf)
    }
    fn flush(&mut self) -> ::std::io::Result<()> {
        use std::io::Write;
        self.stream.borrow_mut().flush()
    }
}

impl<S> HasHandle for Writer<S> where S: ::mio::Evented
{
    fn get_handle(&self) -> Handle {
        self.stream.borrow().handle
    }
}

impl<S> AsyncWrite for Writer<S> where S: ::std::io::Write + ::mio::Evented + 'static
{
    fn write<T>(self, buf: T) -> Promise<(Self, T), Error<(Self, T)>>
        where T: AsRef<[u8]>
    {
        write_internal(self, buf, 0)
    }
}
