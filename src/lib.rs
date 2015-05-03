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

#![allow(dead_code)]

use std::cell::RefCell;
use private::{promise_node, Event, BoolEvent, PromiseAndFulfillerHub,
              EVENT_LOOP, with_current_event_loop, PromiseNode};

pub mod io;

mod private;

pub type Error = Box<::std::error::Error>;
pub type Result<T> = ::std::result::Result<T, Error>;

pub struct Promise<T> where T: 'static {
    pub node : Box<PromiseNode<T>>,
}

impl <T> Promise <T> where T: 'static {
    pub fn then<F, R>(self, func: F) -> Promise<R>
        where F: 'static + FnOnce(T) -> Result<Promise<R>>,
              R: 'static {
                  self.then_else(func, |e| { return Err(e); })
        }

    pub fn then_else<F, G, R>(self, _func: F, _error_handler: G) -> Promise<R>
        where F: 'static + FnOnce(T) -> Result<Promise<R>>,
              G: 'static + FnOnce(Error) -> Result<R>,
              R: 'static {
                  unimplemented!();
        }

    pub fn map<F, R>(self, func: F) -> Promise<R>
        where F: 'static + FnOnce(T) -> Result<R>,
              R: 'static {
            self.map_else(func, |e| { return Err(e); })
        }

    pub fn map_else<F, G, R>(self, func: F, error_handler: G) -> Promise<R>
        where F: 'static + FnOnce(T) -> Result<R>,
              G: 'static + FnOnce(Error) -> Result<R>,
              R: 'static {
            let node : Box<PromiseNode<R>> = Box::new(promise_node::Transform::new(self.node, func, error_handler));
            Promise { node: node }
        }

    pub fn wait(mut self) -> Result<T> {
        with_current_event_loop(move |event_loop| {
            let fired = ::std::rc::Rc::new(::std::cell::Cell::new(false));
            let done_event = BoolEvent::new(fired.clone());
            self.node.on_ready(Box::new(done_event));

            //event_loop.running = true;

            while !fired.get() {
                if !event_loop.turn() {
                    // No events in the queue.
                    panic!("need to implement EventPort");
                }
            }

            self.node.get()
        })
    }

    pub fn fulfilled(value: T) -> Promise<T> {
        return Promise { node: Box::new(promise_node::Immediate::new(Ok(value))) };
    }

    pub fn rejected(error: Error) -> Promise<T> {
        return Promise { node: Box::new(promise_node::Immediate::new(Err(error))) };
    }
}

trait ErrorHandler {
    fn task_failed(error: Error);
}

struct TaskSetImpl {
    error_handler: Box<ErrorHandler>,
}

/// A queue of events being executed in a loop.
pub struct EventLoop {
//    daemons: TaskSetImpl,
    running: bool,
    last_runnable_state: bool,
    events: RefCell<::std::collections::VecDeque<Box<Event>>>,
    depth_first_events: RefCell<::std::collections::VecDeque<Box<Event>>>,
}

impl EventLoop {
    pub fn init() {
        EVENT_LOOP.with(|maybe_event_loop| {
            let event_loop = EventLoop {
                running: false,
                last_runnable_state: false,
                events: RefCell::new(::std::collections::VecDeque::new()),
                depth_first_events: RefCell::new(::std::collections::VecDeque::new()) };

            *maybe_event_loop.borrow_mut() = Some(event_loop);
        });
    }

    fn arm_depth_first(&self, event: Box<Event>) {
        self.depth_first_events.borrow_mut().push_front(event);
    }

    fn arm_breadth_first(&self, event: Box<Event>) {
        self.events.borrow_mut().push_back(event);
    }

    /// Run the event loop for `max_turn_count` turns or until there is nothing left to be done,
    /// whichever comes first. This never calls the `EventPort`'s `sleep()` or `poll()`. It will
    /// call the `EventPort`'s `set_runnable(false)` if the queue becomes empty.
    pub fn run(&mut self, max_turn_count : u32) {
        self.running = true;

        for _ in 0..max_turn_count {
            if !self.turn() {
                break;
            }
        }
    }

    fn turn(&self) -> bool {
        assert!(self.depth_first_events.borrow().is_empty());
        match self.events.borrow_mut().pop_front() {
            None => return false,
            Some(mut event) => {
                // event->firing = true ?
                event.fire();
            }
        }
        while !self.depth_first_events.borrow().is_empty() {
            let event = self.depth_first_events.borrow_mut().pop_back().unwrap();
            self.events.borrow_mut().push_front(event);
        }
        return true;
    }
}

/// A callback which can be used to fulfill a promise.
pub trait PromiseFulfiller<T> where T: 'static {
    fn fulfill(&mut self, value: T);
    fn reject(&mut self, error: Error);
}

pub fn new_promise_and_fulfiller<T>() -> (Promise<T>, Box<PromiseFulfiller<T>>) where T: 'static {
    let result = ::std::rc::Rc::new(::std::cell::RefCell::new(PromiseAndFulfillerHub::new()));
    let result_promise : Promise<T> = Promise { node: Box::new(result.clone())};
    (result_promise, Box::new(result))
}
