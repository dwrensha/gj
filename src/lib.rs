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

extern crate mio;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use private::{promise_node, Event, BoolEvent, PromiseAndFulfillerHub,
              EVENT_LOOP, with_current_event_loop, PromiseNode};

pub mod io;

mod private;
mod handle_table;

pub type Error = Box<::std::error::Error>;
pub type Result<T> = ::std::result::Result<T, Error>;


/// The basic primitive of asynchronous computation in GJ.
pub struct Promise<T> where T: 'static {
    node : Box<PromiseNode<T>>,
}

impl <T> Promise <T> where T: 'static {
    pub fn then<F, R>(self, func: F) -> Promise<R>
        where F: 'static + FnOnce(T) -> Result<Promise<R>>,
              R: 'static {
                  self.then_else(func, |e| { return Err(e); })
        }

    pub fn then_else<F, G, R>(self, func: F, error_handler: G) -> Promise<R>
        where F: 'static + FnOnce(T) -> Result<Promise<R>>,
              G: 'static + FnOnce(Error) -> Result<Promise<R>>,
              R: 'static {
            let intermediate = Box::new(promise_node::Transform::new(self.node, func, error_handler));
            Promise { node: Box::new(promise_node::Chain::new(intermediate)) }
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
            Promise { node: Box::new(promise_node::Transform::new(self.node, func, error_handler)) }
        }

    /// Runs the event loop until the promise is fulfilled.
    ///
    /// The `WaitScope` argument ensures that `wait()` can only be called the top level of a program.
    /// Waiting within event callbacks is disallowed.
    pub fn wait(mut self, _wait_scope: &WaitScope) -> Result<T> {
        with_current_event_loop(move |event_loop| {
            let fired = ::std::rc::Rc::new(::std::cell::Cell::new(false));
            let done_event = BoolEvent::new(fired.clone());
            let (handle, dropper) = private::EventHandle::new(Box::new(done_event));
            self.node.on_ready(handle);

            //event_loop.running = true;

            while !fired.get() {
                if !event_loop.turn() {
                    // No events in the queue.
                    event_loop.event_port.borrow_mut().wait();
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

pub struct WaitScope(::std::marker::PhantomData<*mut u8>); // impl !Sync for WaitScope {}

/// Interface between an `EventLoop` and events originating from outside of the loop's thread.
pub trait EventPort {
    /// Waits for an external event to arrive, sleeping if necessary.
    /// Returns true if wake() has been called from another thread.
    fn wait(&mut self) -> bool;

    /// Checks whether any external events have arrived, but does not sleep.
    /// Returns true if wake() has been called from another thread.
    fn poll(&mut self) -> bool;

    /// Called to notify the `EventPort` when the `EventLoop` has work to do; specifically when it
    /// transitions from empty -> runnable or runnable -> empty. This is typically useful when
    /// intergrating with an external event loop; if the loop is currently runnable then you should
    /// arrange to call run() on it soon. The default implementation does nothing.
    fn set_runnable(&mut self, _runnable: bool) { }


    fn wake(&mut self) { unimplemented!(); }
}

/// A queue of events being executed in a loop.
pub struct EventLoop {
//    daemons: TaskSetImpl,
    event_port: RefCell<io::MioEventPort>,
    running: bool,
    _last_runnable_state: bool,
    events: RefCell<handle_table::HandleTable<private::EventNode>>,
    head: Cell<private::EventHandle>,
    tail: Cell<private::EventHandle>,
    depth_first_insertion_point: Cell<private::EventHandle>,
}



impl EventLoop {
    pub fn top_level<F>(f: F) where F: FnOnce(&WaitScope) {
        let mut events = handle_table::HandleTable::<private::EventNode>::new();
        let dummy = private::EventNode { event: None, next: None, prev: None };
        let head_handle = private::EventHandle(events.push(dummy));

        EVENT_LOOP.with(move |maybe_event_loop| {
            let event_loop = EventLoop {
                event_port: RefCell::new(io::MioEventPort::new().unwrap()),
                running: false,
                _last_runnable_state: false,
                events: RefCell::new(events),
                head: Cell::new(head_handle),
                tail: Cell::new(head_handle),
                depth_first_insertion_point: Cell::new(head_handle), // insert after this node
            };

            assert!(maybe_event_loop.borrow().is_none());
            *maybe_event_loop.borrow_mut() = Some(event_loop);
        });
        let wait_scope = WaitScope(::std::marker::PhantomData );
        f(&wait_scope);
    }

    fn arm_depth_first(&self, event_handle: private::EventHandle) {

        let insertion_node_next = self.events.borrow()[self.depth_first_insertion_point.get().0].next;

        match insertion_node_next {
            Some(next_handle) => {
                self.events.borrow_mut()[next_handle.0].prev = Some(event_handle);
            }
            None => {}
        }

        self.events.borrow_mut()[event_handle.0].prev = Some(self.depth_first_insertion_point.get());
        self.events.borrow_mut()[self.depth_first_insertion_point.get().0].next = Some(event_handle);
        self.depth_first_insertion_point.set(event_handle);
    }

    fn arm_breadth_first(&self, event_handle: private::EventHandle) {
        let events = &mut *self.events.borrow_mut();
        events[self.tail.get().0].next = Some(event_handle);
        events[event_handle.0].prev = Some(self.tail.get());
        self.tail.set(event_handle);
    }

    /// Runs the event loop for `max_turn_count` turns or until there is nothing left to be done,
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

        let mut event_handle = match self.events.borrow()[self.head.get().0].next {
            None => return false,
            Some(event_handle) => { event_handle }
        };
        self.depth_first_insertion_point.set(event_handle);

        // When we're firing, head is the dummy event, as always. head.next is the firing event.
        // depth-first-insertion-point starts out as the firing event.
        // tail is the last event in the queue. If there are no further events,
        // tail is the firing event.

        let mut event = ::std::mem::replace(&mut self.events.borrow_mut()[event_handle.0].event, None)
            .expect("No event to fire?");
        event.fire();

        let event_node = self.events.borrow_mut().remove(event_handle.0).unwrap();

        // event_node.next.prev = event_node.prev
        match event_node.next {
            Some(e) => {
                self.events.borrow_mut()[e.0].prev = event_node.prev;
            }
            None => {}
        }

        // event_node.prev.next = event_node.next
        match event_node.prev {
            Some(e) => {
                self.events.borrow_mut()[e.0].next = event_node.next;
            }
            None => {}
        }

        if self.tail.get() == event_handle {
            self.tail.set(self.head.get());
        }
        return true;
    }
}

/// A callback which can be used to fulfill a promise.
pub trait PromiseFulfiller<T> where T: 'static {
    fn fulfill(self: Box<Self>, value: T);
    fn reject(self: Box<Self>, error: Error);
}

pub fn new_promise_and_fulfiller<T>() -> (Promise<T>, Box<PromiseFulfiller<T>>) where T: 'static {
    let result = ::std::rc::Rc::new(::std::cell::RefCell::new(PromiseAndFulfillerHub::new()));
    let result_promise : Promise<T> = Promise { node: Box::new(result.clone())};
    (result_promise, Box::new(result))
}


/// Holds a collection of Promise<()>s and ensures that each executes to comleteion.
/// Destroying a TaskSet automatically cancels all of its unfinished promises.
pub struct TaskSet {
    task_set_impl: Rc<RefCell<private::TaskSetImpl>>,
}

impl TaskSet {
    pub fn new(error_handler: Box<ErrorHandler>) -> TaskSet {
        TaskSet { task_set_impl : Rc::new(RefCell::new(private::TaskSetImpl::new(error_handler))) }
    }

    pub fn add(&mut self, promise: Promise<()>) {
        private::TaskSetImpl::add(self.task_set_impl.clone(), promise.node);
    }
}

pub trait ErrorHandler {
    fn task_failed(&mut self, error: Error);
}

