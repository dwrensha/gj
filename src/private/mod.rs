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
use std::rc::Rc;
use handle_table::{Handle};
use {Error, Result, PromiseFulfiller, EventLoop, ErrorHandler};

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
    fn on_ready(&mut self, event: EventHandle);

    fn set_self_pointer(&mut self) {}
    fn get(self: Box<Self>) -> Result<T>;
}

pub trait Event {
    fn fire(&mut self);

}

#[derive(PartialEq, Eq, Copy, Clone)]
pub struct EventHandle(pub Handle);

impl EventHandle {
    pub fn new(event: Box<Event>) -> (EventHandle, EventDropper) {
        return with_current_event_loop(|event_loop| {
            let node = EventNode { event: Some(event), next: None, prev: None };
            let handle = EventHandle(event_loop.events.borrow_mut().push(node));
            return (handle, EventDropper { event_handle: handle });
        });
    }
}


/*
impl EventHandle {
    pub fn arm_breadth_first(self: Box<Self>) {
        with_current_event_loop(|event_loop| {
            event_loop.borrow_mut().arm_breadth_first(self);
        });
    }
} */



pub struct EventNode {
    pub event: Option<Box<Event>>,
    pub next: Option<EventHandle>,
    pub prev: Option<EventHandle>
}

pub struct EventDropper {
    event_handle: EventHandle,
}

impl Drop for EventDropper {
    fn drop(&mut self) {
        with_current_event_loop(|event_loop| {
            let maybe_event_node = event_loop.events.borrow_mut().remove(self.event_handle.0);

            match maybe_event_node {
                None => {}
                Some(event_node) => {

                    // event_node.next.prev = event_node.prev
                    match event_node.next {
                        Some(e) => {
                            event_loop.events.borrow_mut()[e.0].prev = event_node.prev;
                        }
                        None => {}
                    }
                    // event_node.prev.next = event_node.next

                    match event_node.prev {
                        Some(e) => {
                            event_loop.events.borrow_mut()[e.0].next = event_node.next;
                        }
                        None => {}
                    }
                }
            }
        });
        // TODO what if it was the tail?
    }
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
    Full(EventHandle),
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

    fn init(&mut self, new_event: EventHandle) {
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

impl <T> PromiseAndFulfillerHub<T> where T: 'static {
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
    fn on_ready(&mut self, event: EventHandle) {
        self.borrow_mut().on_ready_event.init(event);
    }
    fn get(self: Box<Self>) -> Result<T> {
        match ::std::mem::replace(&mut self.borrow_mut().result, None) {
            None => panic!("no result!"),
            Some(r) => r
        }
    }
}

impl <T> PromiseFulfiller<T> for Rc<RefCell<PromiseAndFulfillerHub<T>>> where T: 'static {
    fn fulfill(self: Box<Self>, value: T) {
        self.borrow_mut().fulfill(value);
    }

    fn reject(self: Box<Self>, error: Error) {
        self.borrow_mut().reject(error);
    }
}

pub struct TaskSetImpl {
    error_handler: Box<ErrorHandler>,
}

impl TaskSetImpl {
    pub fn new(error_handler: Box<ErrorHandler>) -> TaskSetImpl {
        TaskSetImpl { error_handler: error_handler }
    }

      pub fn add(task_set: Rc<RefCell<Self>>, mut node: Box<PromiseNode<()>>) {
          let task = Task { task_set: task_set, node: None };
          let (handle, _dropper) = EventHandle::new(Box::new(task));
          node.on_ready(handle);
          unimplemented!()
    }
}

#[allow(dead_code)]
pub struct Task {
    task_set: Rc<RefCell<TaskSetImpl>>,
    node: Option<Box<PromiseNode<()>>>,
}

impl Event for Task {
    fn fire(&mut self) {
        let maybe_node = ::std::mem::replace(&mut self.node, None);
        match maybe_node {
            None => {
                panic!()
            }
            Some(node) => {
                match node.get() {
                    Ok(()) => {}
                    Err(e) => {
                        self.task_set.borrow_mut().error_handler.task_failed(e);
                    }
                }
            }
        }
    }
}

