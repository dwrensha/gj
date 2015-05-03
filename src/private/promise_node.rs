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

use {Result, Error};
use private::{Event, with_current_event_loop, PromiseNode};


/// A PromiseNode that transforms the result of another PromiseNode through an application-provided
/// function (implements `then()`).
pub struct Transform<T, DepT, Func, ErrorFunc>
where Func: FnOnce(DepT) -> Result<T>, ErrorFunc: FnOnce(Error) -> Result<T> {
    dependency: Box<PromiseNode<DepT>>,
    func: Func,
    error_handler: ErrorFunc,
}

impl <T, DepT, Func, ErrorFunc> Transform<T, DepT, Func, ErrorFunc>
where Func: FnOnce(DepT) -> Result<T>, ErrorFunc: FnOnce(Error) -> Result<T> {
    pub fn new(dependency: Box<PromiseNode<DepT>>, func: Func, error_handler: ErrorFunc)
           -> Transform<T, DepT, Func, ErrorFunc> {
        Transform { dependency : dependency,
                    func: func, error_handler: error_handler }
    }
}

impl <T, DepT, Func, ErrorFunc> PromiseNode<T> for Transform<T, DepT, Func, ErrorFunc>
where Func: FnOnce(DepT) -> Result<T>, ErrorFunc: FnOnce(Error) -> Result<T> {
    fn on_ready(&mut self, event: Box<Event>) {
        self.dependency.on_ready(event);
    }
    fn get(self: Box<Self>) -> Result<T> {
        let tmp = *self;
        let Transform {dependency, func, error_handler} = tmp;
        match dependency.get() {
            Ok(value) => {
                func(value)
            }
            Err(e) => {
                error_handler(e)
            }
        }
    }
}


/// A promise that has already been resolved to an immediate value or error.
pub struct Immediate<T> {
    result: Result<T>,
}

impl <T> Immediate<T> {
    pub fn new(result: Result<T>) -> Immediate<T> {
        Immediate { result: result }
    }
}

impl <T> PromiseNode<T> for Immediate<T> {
    fn on_ready(&mut self, event: Box<Event>) {
        with_current_event_loop(|event_loop| {
            event_loop.arm_breadth_first(event);
        });
    }
    fn get(self: Box<Self>) -> Result<T> {
        self.result
    }
}
