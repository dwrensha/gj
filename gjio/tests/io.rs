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

#[macro_use]
extern crate gj;
extern crate gjio;
use gj::{EventLoop, Promise};
use gjio::{Read, Write};

#[test]
fn hello() {
    EventLoop::top_level(|wait_scope| -> Result<(), ::std::io::Error> {
        let mut event_port = try!(gjio::EventPort::new());
        let network = event_port.get_network();
        let addr = ::std::str::FromStr::from_str("127.0.0.1:10000").unwrap();
        let mut address = network.get_tcp_address(addr);
        let mut listener = try!(address.listen());

        let _write_promise = listener.accept().then(move |mut stream| {
            stream.write(vec![0,1,2,3,4,5])
        });

        let read_promise = address.connect().then(move |mut stream| {
            stream.read(vec![0u8; 6], 6)
        });

        let (buf, _) = try!(read_promise.wait(wait_scope, &mut event_port));

        assert_eq!(&buf[..], [0,1,2,3,4,5]);
        Ok(())
    }).expect("top level");
}


#[test]
fn echo() {
    EventLoop::top_level(|wait_scope| -> Result<(), ::std::io::Error> {
        let mut event_port = try!(gjio::EventPort::new());
        let network = event_port.get_network();
        let addr = ::std::str::FromStr::from_str("127.0.0.1:10001").unwrap();
        let mut address = network.get_tcp_address(addr);
        let mut listener = try!(address.listen());

        let _server_promise = listener.accept().then(move |mut stream| {
            stream.read(vec![0u8; 6], 6).lift().then(move |( mut v, _)| {
                assert_eq!(&v[..], [7,6,5,4,3,2]);
                for x in &mut v {
                    *x += 1;
                }
                stream.write(v).lift()
            })
        });

        let client_promise = address.connect().then(move |mut stream| {
            stream.write(vec![7,6,5,4,3,2]).then(move |v| {
                stream.read(v, 6)
            })
        });

        let (buf, _) = try!(client_promise.wait(wait_scope, &mut event_port));
        assert_eq!(&buf[..], [8,7,6,5,4,3]);
        Ok(())
    }).expect("top level");
}

/*
#[cfg(unix)]
#[test]
fn deregister_dupped_unix() {
    use gjmio::unix;
    // At one point, this panicked on Linux with "invalid handle idx".
    EventLoop::top_level(|wait_scope| -> Result<(), ::std::io::Error> {
        let mut event_port = try!(gjmio::EventPort::new());
        let (stream1, stream2) = try!(unix::Stream::new_pair());
        let stream1_dupped = try!(stream1.try_clone());
        drop(stream1);

        let promise1 = stream1_dupped.read(vec![0u8; 6], 6);
        let _promise2 = stream2.write(vec![1,2,3,4,5,6]);

        let _ = promise1.lift::<::std::io::Error>().wait(wait_scope, &mut event_port);
        Ok(())
    }).unwrap();
}


#[test]
fn deregister_dupped_tcp() {
    // At one point, this panicked on Linux with "invalid handle idx".
    EventLoop::top_level(|wait_scope| -> Result<(), ::std::io::Error> {
        let mut event_port = try!(gjmio::EventPort::new());
        let addr = ::std::str::FromStr::from_str("127.0.0.1:10002").unwrap();
        let listener = tcp::Listener::bind(addr).unwrap();

        let server_promise = listener.accept().lift::<::std::io::Error>().map(move |(_, stream1)| {
            Ok(Some(stream1))
        });
        let client_promise = tcp::Stream::connect(addr).lift::<::std::io::Error>().map(|stream2| Ok(Some(stream2)));
        let promise = Promise::all(vec![server_promise, client_promise].into_iter()).then(|mut streams| {
            let stream0 = streams[0].take().unwrap();
            let stream1 = streams[1].take().unwrap();
            let stream0_dupped = pry!(stream0.try_clone());
            drop(stream0);

            let promise1 = stream0_dupped.read(vec![0u8; 6], 6).lift();
            let promise2 = stream1.write(vec![1,2,3,4,5,6]).lift();

            promise2.then(|_| promise1)
        });

        let _ = promise.wait(wait_scope, &mut event_port);
        Ok(())
    }).unwrap();
}
*/
