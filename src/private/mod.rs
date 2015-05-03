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

use std::cell::RefCell;
use {Error, Result, PromiseFulfiller, EventLoop};

pub mod promise_node;

thread_local!(pub static EVENT_LOOP: RefCell<Option<EventLoop>> = RefCell::new(None));

pub fn with_current_event_loop<F, R>(f: F) -> R
    where F: FnOnce(&EventLoop) -> R {
        EVENT_LOOP.with(|maybe_event_loop| {
            match &*maybe_event_loop.borrow() {
                &None => panic!("current thread has no event loop"),
                &Some(ref event_loop) => f(event_loop),
            }
        })
    }

pub trait PromiseNode<T> {
    /// Arms the given event when the promised value is ready.
    fn on_ready(&mut self, event: Box<Event>);

    fn set_self_pointer(&mut self) {}
    fn get(self: Box<Self>) -> Result<T>;
}

pub trait Event {
    fn fire(&mut self);


    /* TODO why doesn't this work? Something about Sized?
    fn arm_breadth_first(self: Box<Self>) {
        with_current_event_loop(|event_loop| {
     //       event_loop.borrow_mut().arm_breadth_first(self);
        });
    } */
}

pub struct BoolEvent {
    fired: ::std::rc::Rc<::std::cell::Cell<bool>>,
}

impl BoolEvent {
    pub fn new(fired: ::std::rc::Rc<::std::cell::Cell<bool>>) -> BoolEvent {
        BoolEvent { fired: fired }
    }
}

impl Event for BoolEvent {
    fn fire(&mut self) {
        self.fired.set(true);
    }
}

pub enum OnReadyEvent {
    Empty,
    AlreadyReady,
    Full(Box<Event>),
}

impl OnReadyEvent {
    fn is_already_ready(&self) -> bool {
        match self {
            &OnReadyEvent::AlreadyReady => return true,
            _ => return false,
        }
    }

    fn is_empty(&self) -> bool {
        match self {
            &OnReadyEvent::Empty => return true,
            _ => return false,
        }
    }

    fn init(&mut self, new_event: Box<Event>) {
        with_current_event_loop(|event_loop| {
            if self.is_already_ready() {
                event_loop.arm_breadth_first(new_event);
            } else {
                *self = OnReadyEvent::Full(new_event);
            }
        });
    }

    fn arm(&mut self) {
        if self.is_empty() {
            *self = OnReadyEvent::AlreadyReady;
        } else {
            let old_self = ::std::mem::replace(self, OnReadyEvent::Empty);
            match old_self {
                OnReadyEvent::Full(event) => {
                    with_current_event_loop(|event_loop| {
                        event_loop.arm_depth_first(event);
                    });
                }
                _ => {
                    panic!("armed an event twice?");
                }
            }
        }
    }
}

pub struct PromiseAndFulfillerHub<T> where T: 'static {
    result: Option<Result<T>>,
    on_ready_event: OnReadyEvent,
}

impl <T> PromiseAndFulfillerHub<T> where T: 'static {
    pub fn new() -> PromiseAndFulfillerHub<T> {
        PromiseAndFulfillerHub { result: None::<Result<T>>, on_ready_event: OnReadyEvent::Empty }
    }
}

impl <T> PromiseFulfiller<T> for PromiseAndFulfillerHub<T> where T: 'static {
    fn fulfill(&mut self, value: T) {
        if self.result.is_none() {
            self.result = Some(Ok(value));
        }
        self.on_ready_event.arm();
    }

    fn reject(&mut self, error: Error) {
        if self.result.is_none() {
            self.result = Some(Err(error));
        }
    }
}

/*
pub struct WrapperPromiseNode<T> where T: 'static {
    hub: ::std::rc::Rc<::std::cell::RefCell<PromiseAndFulfillerHub<T>>>,
}
*/

impl <T> PromiseNode<T> for ::std::rc::Rc<::std::cell::RefCell<PromiseAndFulfillerHub<T>>> where T: 'static {
    fn on_ready(&mut self, event: Box<Event>) {
        self.borrow_mut().on_ready_event.init(event);
    }
    fn get(self: Box<Self>) -> Result<T> {
        match ::std::mem::replace(&mut self.borrow_mut().result, None) {
            None => panic!("no result!"),
            Some(r) => r
        }
    }
}

impl <T> PromiseFulfiller<T> for ::std::rc::Rc<::std::cell::RefCell<PromiseAndFulfillerHub<T>>> where T: 'static {
    fn fulfill(&mut self, value: T) {
        self.borrow_mut().fulfill(value);
    }

    fn reject(&mut self, error: Error) {
        self.borrow_mut().reject(error);
    }
}
