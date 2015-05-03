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
fn eval_void() {
    use std::rc::Rc;
    use std::cell::Cell;
    ::gj::EventLoop::init();
    let done = Rc::new(Cell::new(false));
    let done1 = done.clone();
    let promise = ::gj::Promise::fulfilled(()).map(move |()| {
        done1.clone().set(true);
        return Ok(());
    });
    assert_eq!(done.get(), false);
    promise.wait().unwrap();
    assert_eq!(done.get(), true);
}

#[test]
fn eval_int() {
    ::gj::EventLoop::init();
    let promise = ::gj::Promise::fulfilled(19u64).map(|x| {
        assert_eq!(x, 19);
        return Ok(x + 2);
    });
    let value = promise.wait().unwrap();
    assert_eq!(value, 21);
}


#[test]
fn fulfiller() {
    ::gj::EventLoop::init();
    let (promise, mut fulfiller) = ::gj::new_promise_and_fulfiller::<u32>();
    let p1 = promise.map(|x| {
        assert_eq!(x, 10);
        return Ok(x + 1);
    });

    fulfiller.fulfill(10);
    let value = p1.wait().unwrap();
    assert_eq!(value, 11);

}

#[test]
fn chain() {
    gj::EventLoop::init();

    let promise: gj::Promise<i32> = gj::Promise::fulfilled(()).map(|()| { return Ok(123); });
    let promise2: gj::Promise<i32> = gj::Promise::fulfilled(()).map(|()| { return Ok(321); });

    let promise3 = promise.then(move |i| {
        return Ok(promise2.map(move |j| {
            return Ok(i + j);
        }));
    });

    //let value = promise3.wait().unwrap();
    //assert_eq!(444, value);
}
