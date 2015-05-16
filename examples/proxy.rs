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

fn forward<R,W,B>(src: R, dst: W, buf: B) -> gj::Promise<()>
    where R: AsyncRead, W: AsyncWrite, B: ::std::ops::DerefMut<Target=[u8]> + 'static
{
    return src.try_read(buf, 1).then(move |(src, buf, n)| {
        if n == 0 {
            // EOF
            return Ok(gj::Promise::fulfilled(()));
        } else {
            return Ok(dst.write(gj::io::Slice::new(buf, n)).then(move |(dst, slice)| {
                return Ok(forward(src, dst, slice.buf));
            }));
        }
    });
}

fn accept_loop(receiver: gj::io::ConnectionReceiver,
               outbound_addr: gj::io::NetworkAddress,
               mut task_set: Vec<gj::Promise<()>>) -> gj::Promise<()> {

    return receiver.accept().then(move |(receiver, (src_tx, src_rx))| {
        println!("handling connection");

        return Ok(outbound_addr.connect().then(move |(dst_tx, dst_rx)| {
            task_set.push(forward(src_rx, dst_tx, vec![0; 1024]));
            task_set.push(forward(dst_rx, src_tx, vec![0; 1024]));
            return Ok(accept_loop(receiver, outbound_addr, task_set));
        }));
    });
}

pub fn main() {
    gj::EventLoop::init(|wait_scope| {

        let addr = gj::io::NetworkAddress::new("127.0.0.1:9999").unwrap();
        let receiver = addr.listen().unwrap();

        // Hard coded to a google IP
        let outbound_addr = gj::io::NetworkAddress::new("216.58.216.164:80").unwrap();

        accept_loop(receiver, outbound_addr, Vec::new()).wait(wait_scope).unwrap();
    });
}
