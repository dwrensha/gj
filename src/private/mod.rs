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
use std::rc::{Rc, Weak};
use std::collections::HashMap;
use std::result::Result;
use handle_table::{Handle};
use {EventLoop, TaskReaper};

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

    /// Tells the node that `_self_ptr` is the pointer that owns this node, and will continue to own
    /// this node until it is destroyed or set_self_pointer() is called again. promise_node::Chain uses
    /// this to shorten redundant chains.  The default implementation does nothing; only
    /// promise_node::Chain should implement this.
    fn set_self_pointer(&mut self, _self_ptr: Weak<RefCell<promise_node::ChainState<T, E>>>) {}
    fn get(self: Box<Self>) -> Result<T, E>;
}

pub trait Event {
    fn fire(&mut self) -> Option<Box<OpaqueEventDropper>>;
}

#[derive(PartialEq, Eq, Copy, Clone, Hash)]
pub struct EventHandle(pub Handle);

impl EventHandle {
    pub fn new() -> (EventHandle, EventDropper) {
        with_current_event_loop(|event_loop| {
            let node = EventNode { event: None, next: None, prev: None };
            let handle = EventHandle(event_loop.events.borrow_mut().push(node));
            (handle, EventDropper { event_handle: handle })
        })
    }

    pub fn set(&self, event: Box<Event>) {
        with_current_event_loop(|event_loop| {
            event_loop.events.borrow_mut()[self.0].event = Some(event);
        })
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
            match event_loop.currently_firing.get() {
                Some(h) if h == self.event_handle => panic!("Promise callback destroyed itself."),
                _ => (),
            }
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

                    let insertion_point = event_loop.depth_first_insertion_point.get();
                    if insertion_point.0 == self.event_handle.0 {
                        event_loop.depth_first_insertion_point.set(event_node.prev.unwrap());
                    }

                    let tail = event_loop.tail.get();
                    if tail.0 == self.event_handle.0 {
                        event_loop.tail.set(event_node.prev.unwrap());
                    }
                }
            }
        });
    }
}

pub trait OpaqueEventDropper {}

impl <T, E> OpaqueEventDropper for Box<PromiseNode<T, E>> {}

impl OpaqueEventDropper for EventDropper {}

pub struct BoolEvent {
    fired: ::std::rc::Rc<::std::cell::Cell<bool>>,
}

impl BoolEvent {
    pub fn new(fired: ::std::rc::Rc<::std::cell::Cell<bool>>) -> BoolEvent {
        BoolEvent { fired: fired }
    }
}

impl Event for BoolEvent {
    fn fire(&mut self) -> Option<Box<OpaqueEventDropper>> {
        self.fired.set(true);
        None
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
            &OnReadyEvent::AlreadyReady => true,
            _ => false,
        }
    }

    fn is_empty(&self) -> bool {
        match self {
            &OnReadyEvent::Empty => true,
            _ => false,
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

pub struct PromiseAndFulfillerHub<T, E> where T: 'static, E: 'static {
    result: Option<Result<T, E>>,
    on_ready_event: OnReadyEvent,
}

impl <T, E> PromiseAndFulfillerHub<T, E> {
    pub fn new() -> PromiseAndFulfillerHub<T, E> {
        PromiseAndFulfillerHub { result: None::<Result<T, E>>, on_ready_event: OnReadyEvent::Empty }
    }
}

impl <T, E> PromiseAndFulfillerHub<T, E> {
    pub fn fulfill(&mut self, value: T) {
        if self.result.is_none() {
            self.result = Some(Ok(value));
            self.on_ready_event.arm();
        }
    }

    pub fn reject(&mut self, error: E) {
        if self.result.is_none() {
            self.result = Some(Err(error));
            self.on_ready_event.arm();
        }
    }
}

pub struct PromiseAndFulfillerWrapper<T, E> where T: 'static, E: 'static {
    hub: ::std::rc::Rc<::std::cell::RefCell<PromiseAndFulfillerHub<T, E>>>
}

impl <T, E> PromiseAndFulfillerWrapper<T, E> {
    pub fn new(hub: ::std::rc::Rc<::std::cell::RefCell<PromiseAndFulfillerHub<T, E>>>)
               -> PromiseAndFulfillerWrapper<T, E>
    {
        PromiseAndFulfillerWrapper { hub: hub }
    }
}

impl <T, E> Drop for PromiseAndFulfillerWrapper<T, E> {
    fn drop(&mut self) {
        self.hub.borrow_mut().on_ready_event = OnReadyEvent::Empty;
    }
}

impl <T, E> PromiseNode<T, E> for PromiseAndFulfillerWrapper<T, E> {
    fn on_ready(&mut self, event: EventHandle) {
        self.hub.borrow_mut().on_ready_event.init(event);
    }
    fn get(self: Box<Self>) -> Result<T, E> {
        match ::std::mem::replace(&mut self.hub.borrow_mut().result, None) {
            None => panic!("no result!"),
            Some(r) => r
        }
    }
}

pub struct TaskSetImpl<T, E> where T: 'static, E: 'static {
    reaper: Rc<RefCell<Box<TaskReaper<T, E>>>>,
    tasks: Rc<RefCell<HashMap<EventHandle, EventDropper>>>,
}

impl <T, E> TaskSetImpl <T, E> {
    pub fn new(reaper: Box<TaskReaper<T, E>>) -> TaskSetImpl<T, E> {
        TaskSetImpl {
            reaper: Rc::new(RefCell::new(reaper)),
            tasks: Rc::new(RefCell::new(HashMap::new())),
        }
    }

      pub fn add(&self, mut node: Box<PromiseNode<T, E>>) {
          let (handle, dropper) = EventHandle::new();
          node.on_ready(handle);
          let task = Task {
              weak_reaper: Rc::downgrade(&self.reaper),
              weak_tasks: Rc::downgrade(&self.tasks),
              node: Some(node),
              event_handle: handle
          };
          handle.set(Box::new(task));
          self.tasks.borrow_mut().insert(handle, dropper);
    }
}

pub struct Task<T, E> where T: 'static, E: 'static {
    weak_reaper: Weak<RefCell<Box<TaskReaper<T, E>>>>,
    weak_tasks: Weak<RefCell<HashMap<EventHandle, EventDropper>>>,
    node: Option<Box<PromiseNode<T, E>>>,
    event_handle: EventHandle,
}

impl <T, E> Event for Task<T, E> {
    fn fire(&mut self) -> Option<Box<OpaqueEventDropper>> {
        let maybe_node = ::std::mem::replace(&mut self.node, None);
        match maybe_node {
            None => {
                unreachable!()
            }
            Some(node) => {
                match self.weak_reaper.upgrade() {
                    None => (),
                    Some(reaper) => {
                        match node.get() {
                            Ok(v) => {
                                reaper.borrow_mut().task_succeeded(v);
                            }
                            Err(e) => {
                                reaper.borrow_mut().task_failed(e);
                            }
                        }
                    }
                }
            }
        }
        let tasks = self.weak_tasks.upgrade().expect("dangling reference to tasks?");
        let mut tmp = tasks.borrow_mut();
        match tmp.remove(&self.event_handle) {
            None => None,
            Some(v) => Some(Box::new(v)),
        }
    }
}
