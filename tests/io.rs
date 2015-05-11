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

#[test]
fn hello() {
    use gj::io::{AsyncRead, AsyncWrite};
    gj::EventLoop::init();

    let addr = gj::io::NetworkAddress::new(::std::str::FromStr::from_str("127.0.0.1:10000").unwrap());

    let receiver = addr.listen().unwrap();

    let _write_promise = receiver.accept().then(move |(_, stream)| {
        return Ok(stream.write(vec![0,1,2,3,4,5]));
    });

    let read_promise = addr.connect().then(move |stream| {
        return Ok(stream.read(vec![0u8; 6], 6, 6));
    });

    let (_, buf, _) = read_promise.wait().unwrap();

    assert_eq!(&buf[..], [0,1,2,3,4,5]);
}
