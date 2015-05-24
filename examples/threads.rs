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

extern crate gj;
use gj::io::{AsyncRead, AsyncWrite};

fn child_loop(delay: u32, stream: gj::io::SocketStream, buf: Vec<u8>) -> gj::Promise<()> {

    // This blocks the entire thread. This is okay because we are on a child thread
    // where nothing else needs to happen.
    ::std::thread::sleep_ms(delay);

    return stream.write(buf).then(move |(stream, buf)| {
        return Ok(child_loop(delay, stream, buf));
    });
}

fn child(delay: u32) -> gj::Result<gj::io::SocketStream> {
    let (_, stream) = try!(gj::io::spawn(move |parent_stream, wait_scope| {
        try!(child_loop(delay, parent_stream, vec![0u8]).wait(wait_scope));
        Ok(())
    }));
    return Ok(stream);
}

fn listen_to_child(id: String, stream: gj::io::SocketStream, buf: Vec<u8>) -> gj::Promise<()> {
    return stream.read(buf, 1).then(move |(stream, buf, _n)| {
        println!("heard back from {}", id);
        return Ok(listen_to_child(id, stream, buf));
    });
}

fn parent_wait_loop() -> gj::Promise<()> {
    println!("parent wait loop...");

    // If we used ::std::thread::sleep_ms() here, we would block the main event loop.
    return gj::io::Timer.after_delay_ms(3000).then(|()| {
        return Ok(parent_wait_loop());
    });
}

pub fn main() {
    gj::EventLoop::top_level(|wait_scope| {

        let children = vec![
            parent_wait_loop(),
            listen_to_child("CHILD 1".to_string(), try!(child(700)), vec![0]),
            listen_to_child("CHILD 2".to_string(), try!(child(1900)), vec![0]),
            listen_to_child("CHILD 3".to_string(), try!(child(2600)), vec![0])];

        try!(gj::join_promises(children).wait(wait_scope));

        Ok(())
    }).unwrap();
}
