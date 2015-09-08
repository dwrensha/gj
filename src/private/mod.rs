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
use std::collections::HashMap;
use std::result::Result;
use handle_table::{Handle};
use {PromiseFulfiller, EventLoop, ErrorHandler, Forced};

pub mod promise_node;

thread_local!(pub static EVENT_LOOP: RefCell<Option<EventLoop>> = RefCell::new(None));

pub fn with_current_event_loop<F, R>(f: F) -> R
    where F: FnOnce(&EventLoop) -> R
{
    EVENT_LOOP.with(|maybe_event_loop| {
        match &*maybe_event_loop.borrow() {
            &None => panic!("current thread has no event loop"),
            &Some(ref event_loop) => f(event_loop),
        }
    })
}

pub trait PromiseNode<T, E> {
    /// Arms the given event when the promised value is ready.
    fn on_ready(&mut self, event: EventHandle);

    fn set_self_pointer(&mut self) {}
    fn get(self: Box<Self>) -> Result<T, E>;
    fn force(self: Box<Self>) -> Result<T, E>;
}

pub trait Event {
    fn fire(&mut self) -> Option<EventDropper>;
}

#[derive(PartialEq, Eq, Copy, Clone, Hash)]
pub struct EventHandle(pub Handle);

impl EventHandle {
    pub fn new() -> (EventHandle, EventDropper) {
        return with_current_event_loop(|event_loop| {
            let node = EventNode { event: None, next: None, prev: None };
            let handle = EventHandle(event_loop.events.borrow_mut().push(node));
            return (handle, EventDropper { event_handle: handle });
        });
    }

    pub fn set(&self, event: Box<Event>) {
        return with_current_event_loop(|event_loop| {
            event_loop.events.borrow_mut()[self.0].event = Some(event);
        });
    }

    pub fn arm_breadth_first(self) {
        with_current_event_loop(|event_loop| {
            event_loop.arm_breadth_first(self);
        });
    }

    pub fn arm_depth_first(self) {
        with_current_event_loop(|event_loop| {
            event_loop.arm_depth_first(self);
        });
    }
}

pub struct EventNode {
    pub event: Option<Box<Event>>,
    pub next: Option<EventHandle>,
    pub prev: Option<EventHandle>
}

#[derive(PartialEq, Eq, Hash)]
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
                            event_loop.tail.set(e);
                        }
                        None => {}
                    }
                }
            }
        });
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
    fn fire(&mut self) -> Option<EventDropper> {
        self.fired.set(true);
        return None;
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
        if self.is_already_ready() {
            new_event.arm_breadth_first();
        } else {
            *self = OnReadyEvent::Full(new_event);
        }
    }

    fn arm(&mut self) {
        if self.is_empty() {
            *self = OnReadyEvent::AlreadyReady;
        } else {
            let old_self = ::std::mem::replace(self, OnReadyEvent::Empty);
            match old_self {
                OnReadyEvent::Full(event) => {
                    event.arm_depth_first();
                }
                _ => {
                    panic!("armed an event twice?");
                }
            }
        }
    }
}

pub struct PromiseAndFulfillerHub<T, E> where T: 'static, E: 'static + From<Forced> {
    result: Option<Result<T, E>>,
    on_ready_event: OnReadyEvent,
}

impl <T, E> PromiseAndFulfillerHub<T, E> where E: From<Forced> {
    pub fn new() -> PromiseAndFulfillerHub<T, E> {
        PromiseAndFulfillerHub { result: None::<Result<T, E>>, on_ready_event: OnReadyEvent::Empty }
    }
}

impl <T, E> PromiseAndFulfillerHub<T, E> where E: From<Forced> {
    fn fulfill(&mut self, value: T) {
        if self.result.is_none() {
            self.result = Some(Ok(value));
        }
        self.on_ready_event.arm();
    }

    fn reject(&mut self, error: E) {
        if self.result.is_none() {
            self.result = Some(Err(error));
        }
    }
}

impl <T, E> PromiseNode<T, E> for ::std::rc::Rc<::std::cell::RefCell<PromiseAndFulfillerHub<T, E>>>
    where E: From<Forced>
{
    fn on_ready(&mut self, event: EventHandle) {
        self.borrow_mut().on_ready_event.init(event);
    }
    fn get(self: Box<Self>) -> Result<T, E> {
        match ::std::mem::replace(&mut self.borrow_mut().result, None) {
            None => panic!("no result!"),
            Some(r) => r
        }
    }
    fn force(self: Box<Self>) -> Result<T, E> {
        Err(E::from(Forced))
    }
}

impl <T, E> PromiseFulfiller<T, E> for Rc<RefCell<PromiseAndFulfillerHub<T, E>>>
    where T: 'static, E: 'static + From<Forced>
{
    fn fulfill(self: Box<Self>, value: T) {
        self.borrow_mut().fulfill(value);
    }

    fn reject(self: Box<Self>, error: E) {
        self.borrow_mut().reject(error);
    }
}

pub struct TaskSetImpl {
    error_handler: Box<ErrorHandler>,
    tasks: HashMap<EventHandle, EventDropper>,
}

impl TaskSetImpl {
    pub fn new(error_handler: Box<ErrorHandler>) -> TaskSetImpl {
        TaskSetImpl { error_handler: error_handler,
                      tasks: HashMap::new() }
    }

      pub fn add(task_set: Rc<RefCell<Self>>, mut node: Box<PromiseNode<(), Box<::std::error::Error>>>) {
          let (handle, dropper) = EventHandle::new();
          node.on_ready(handle);
          let task = Task { task_set: task_set.clone(), node: Some(node), event_handle: handle };
          handle.set(Box::new(task));
          task_set.borrow_mut().tasks.insert(handle, dropper);
    }
}

pub struct Task {
    task_set: Rc<RefCell<TaskSetImpl>>,
    node: Option<Box<PromiseNode<(), Box<::std::error::Error>>>>,
    event_handle: EventHandle,
}

impl Event for Task {
    fn fire(&mut self) -> Option<EventDropper> {
        let maybe_node = ::std::mem::replace(&mut self.node, None);
        match maybe_node {
            None => {
                panic!()
            }
            Some(node) => {
                match node.get() {
                    Ok(()) => {
                        return self.task_set.borrow_mut().tasks.remove(&self.event_handle);
                    }
                    Err(e) => {
                        self.task_set.borrow_mut().error_handler.task_failed(e);
                        return None;
                    }
                }
            }
        }
    }
}

