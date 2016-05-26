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

//! A library providing high-level abstractions for event loop concurrency,
//! heavily borrowing ideas from [KJ](https://capnproto.org/cxxrpc.html#kj-concurrency-framework).
//! Allows for coordination of asynchronous tasks using [promises](struct.Promise.html) as
//! a basic building block.
//!
//! # Example
//!
//! ```
//! use gj::{EventLoop, Promise, ClosedEventPort};
//! EventLoop::top_level(|wait_scope| -> Result<(),()> {
//!     let (promise1, fulfiller1) = Promise::<(),()>::and_fulfiller();
//!     let (promise2, fulfiller2) = Promise::<(),()>::and_fulfiller();
//!     let promise3 = promise2.then(|_| {
//!         println!("world");
//!         Promise::ok(())
//!     });
//!     let promise4 = promise1.then(move |_| {
//!         println!("hello ");
//!         fulfiller2.fulfill(());
//!         Promise::ok(())
//!     });
//!     fulfiller1.fulfill(());
//!     Promise::all(vec![promise3, promise4].into_iter())
//!         .map(|_| Ok(()))
//!         .wait(wait_scope, &mut ClosedEventPort(()))
//! }).expect("top level");
//! ```


use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::result::Result;
use private::{promise_node, BoolEvent, PromiseAndFulfillerHub, PromiseAndFulfillerWrapper,
              EVENT_LOOP, with_current_event_loop, PromiseNode};


/// Like `try!()`, but for functions that return a [`Promise<T, E>`](struct.Promise.html) rather
/// than a `Result<T, E>`.
///
/// Unwraps a `Result<T, E>`. In the case of an error `Err(e)`, immediately returns from the
/// enclosing function with `Promise::err(e)`.
#[macro_export]
macro_rules! pry {
    ($expr:expr) => (match $expr {
        ::std::result::Result::Ok(val) => val,
        ::std::result::Result::Err(err) => {
            return $crate::Promise::err(::std::convert::From::from(err))
        }
    })
}

mod private;
mod handle_table;

/// A computation that might eventually resolve to a value of type `T` or to an error
///  of type `E`. Dropping the promise cancels the computation.
#[must_use]
pub struct Promise<T, E>
    where T: 'static,
          E: 'static
{
    node: Box<PromiseNode<T, E>>,
}

impl<T, E> Promise<T, E> {
    /// Creates a new promise that has already been fulfilled with the given value.
    pub fn ok(value: T) -> Promise<T, E> {
        Promise { node: Box::new(promise_node::Immediate::new(Ok(value))) }
    }

    /// Creates a new promise that has already been rejected with the given error.
    pub fn err(error: E) -> Promise<T, E> {
        Promise { node: Box::new(promise_node::Immediate::new(Err(error))) }
    }

    /// Creates a new promise that never resolves.
    pub fn never_done() -> Promise<T, E> {
        Promise { node: Box::new(promise_node::NeverDone::new()) }
    }

    /// Creates a new promise/fulfiller pair.
    pub fn and_fulfiller() -> (Promise<T, E>, PromiseFulfiller<T, E>)
        where E: FulfillerDropped
    {
        let result = Rc::new(RefCell::new(PromiseAndFulfillerHub::new()));
        let result_promise: Promise<T, E> = Promise {
            node: Box::new(PromiseAndFulfillerWrapper::new(result.clone())),
        };
        (result_promise,
         PromiseFulfiller {
            hub: result,
            done: false,
        })
    }

    /// Chains further computation to be executed once the promise resolves.
    /// When the promise is fulfilled or rejected, invokes `func` on its result.
    ///
    /// If the returned promise is dropped before the chained computation runs, the chained
    /// computation will be cancelled.
    ///
    /// Always returns immediately, even if the promise is already resolved. The earliest that
    /// `func` might be invoked is during the next `turn()` of the event loop.
    pub fn then_else<F, T1, E1>(self, func: F) -> Promise<T1, E1>
        where F: 'static,
              F: FnOnce(Result<T, E>) -> Promise<T1, E1>
    {
        let intermediate = Box::new(promise_node::Transform::new(self.node, |x| Ok(func(x))));
        Promise { node: Box::new(promise_node::Chain::new(intermediate)) }
    }

    /// Calls `then_else()` with a default error handler that simply propagates all errors.
    pub fn then<F, T1>(self, func: F) -> Promise<T1, E>
        where F: 'static,
              F: FnOnce(T) -> Promise<T1, E>
    {
        self.then_else(|r| {
            match r {
                Ok(v) => func(v),
                Err(e) => Promise::err(e),
            }
        })
    }

    /// Like `then_else()` but for a `func` that returns a direct value rather than a promise. As an
    /// optimization, execution of `func` is delayed until its result is known to be needed. The
    /// expectation here is that `func` is just doing some transformation on the results, not
    /// scheduling any other actions, and therefore the system does not need to be proactive about
    /// evaluating it. This way, a chain of trivial `map()` transformations can be executed all at
    /// once without repeated rescheduling through the event loop. Use the `eagerly_evaluate()`
    /// method to suppress this behavior.
    pub fn map_else<F, T1, E1>(self, func: F) -> Promise<T1, E1>
        where F: 'static,
              F: FnOnce(Result<T, E>) -> Result<T1, E1>
    {
        Promise { node: Box::new(promise_node::Transform::new(self.node, func)) }
    }

    /// Calls `map_else()` with a default error handler that simply propagates all errors.
    pub fn map<F, R>(self, func: F) -> Promise<R, E>
        where F: 'static,
              F: FnOnce(T) -> Result<R, E>,
              R: 'static
    {
        self.map_else(|r| {
            match r {
                Ok(v) => func(v),
                Err(e) => Err(e),
            }
        })
    }

    /// Transforms the error branch of the promise.
    pub fn map_err<E1, F>(self, func: F) -> Promise<T, E1>
        where F: 'static,
              F: FnOnce(E) -> E1
    {
        self.map_else(|r| {
            match r {
                Ok(v) => Ok(v),
                Err(e) => Err(func(e)),
            }
        })
    }

    /// Maps errors into a more general type.
    pub fn lift<E1>(self) -> Promise<T, E1>
        where E: Into<E1>
    {
        self.map_err(|e| e.into())
    }

    /// Returns a new promise that resolves when either `self` or `other` resolves. The promise that
    /// doesn't resolve first is cancelled.
    pub fn exclusive_join(self, other: Promise<T, E>) -> Promise<T, E> {
        Promise { node: Box::new(private::promise_node::ExclusiveJoin::new(self.node, other.node)) }
    }

    /// Transforms a collection of promises into a promise for a vector. If any of
    /// the promises fails, immediately cancels the remaining promises.
    pub fn all<I>(promises: I) -> Promise<Vec<T>, E>
        where I: Iterator<Item = Promise<T, E>>
    {
        Promise { node: Box::new(private::promise_node::ArrayJoin::new(promises)) }
    }

    /// Forks the promise, so that multiple different clients can independently wait on the result.
    pub fn fork(self) -> ForkedPromise<T, E>
        where T: Clone,
              E: Clone
    {
        ForkedPromise::new(self)
    }

    /// Holds onto `value` until the promise resolves, then drops `value`.
    pub fn attach<U>(self, value: U) -> Promise<T, E>
        where U: 'static
    {
        self.map_else(move |result| {
            drop(value);
            result
        })
    }

    /// Forces eager evaluation of this promise. Use this if you are going to hold on to the promise
    /// for a while without consuming the result, but you want to make sure that the system actually
    /// processes it.
    pub fn eagerly_evaluate(self) -> Promise<T, E> {
        self.then(|v| Promise::ok(v))
    }

    /// Runs the event loop until the promise is fulfilled.
    ///
    /// The `WaitScope` argument ensures that `wait()` can only be called at the top level of a
    /// program. Waiting within event callbacks is disallowed.
    pub fn wait<E1>(mut self,
                    wait_scope: &WaitScope,
                    event_source: &mut EventPort<E1>)
                    -> Result<T, E>
        where E: From<E1>
    {
        drop(wait_scope);
        with_current_event_loop(move |event_loop| {
            let fired = ::std::rc::Rc::new(::std::cell::Cell::new(false));
            let done_event = BoolEvent::new(fired.clone());
            let (handle, _dropper) = private::GuardedEventHandle::new();
            handle.set(Box::new(done_event));
            self.node.on_ready(handle);

            while !fired.get() {
                if !event_loop.turn() {
                    // No events in the queue.
                    try!(event_source.wait());
                }
            }

            self.node.get()
        })
    }
}

/// A scope in which asynchronous programming can occur. Corresponds to the top level scope of some
/// [event loop](struct.EventLoop.html). Can be used to [wait](struct.Promise.html#method.wait) for
/// the result of a promise.
pub struct WaitScope(::std::marker::PhantomData<*mut u8>); // impl !Sync for WaitScope {}

/// The result of `Promise::fork()`. Allows branches to be created. Dropping the `ForkedPromise`
/// along with any branches created through `add_branch()` will cancel the computation.
pub struct ForkedPromise<T, E>
    where T: 'static + Clone,
          E: 'static + Clone
{
    hub: Rc<RefCell<promise_node::ForkHub<T, E>>>,
}

impl<T, E> ForkedPromise<T, E>
    where T: 'static + Clone,
          E: 'static + Clone
{
    fn new(inner: Promise<T, E>) -> ForkedPromise<T, E> {
        ForkedPromise { hub: Rc::new(RefCell::new(promise_node::ForkHub::new(inner.node))) }
    }

    /// Creates a new promise that will resolve when the originally forked promise resolves.
    pub fn add_branch(&mut self) -> Promise<T, E> {
        promise_node::ForkHub::add_branch(&self.hub)
    }
}

/// Interface between an `EventLoop` and events originating from outside of the loop's thread.
/// Needed in [`Promise::wait()`](struct.Promise.html#method.wait).
pub trait EventPort<E> {
    /// Waits for an external event to arrive, blocking the thread if necessary.
    fn wait(&mut self) -> Result<(), E>;
}

/// An event port that never emits any events. On wait() it returns the error it was constructed
/// with.
pub struct ClosedEventPort<E: Clone>(pub E);

impl<E: Clone> EventPort<E> for ClosedEventPort<E> {
    fn wait(&mut self) -> Result<(), E> {
        Err(self.0.clone())
    }
}

/// A queue of events being executed in a loop on a single thread.
pub struct EventLoop {
    // daemons: TaskSetImpl,
    _last_runnable_state: bool,
    events: RefCell<handle_table::HandleTable<private::EventNode>>,
    head: private::EventHandle,
    tail: Cell<private::EventHandle>,
    depth_first_insertion_point: Cell<private::EventHandle>,
    currently_firing: Cell<Option<private::EventHandle>>,
    to_destroy: Cell<Option<private::EventHandle>>,
}

impl EventLoop {
    /// Creates an event loop for the current thread, panicking if one already exists. Runs the
    /// given closure and then drops the event loop.
    pub fn top_level<R, F>(main: F) -> R
        where F: FnOnce(&WaitScope) -> R
    {
        let mut events = handle_table::HandleTable::<private::EventNode>::new();
        let dummy = private::EventNode {
            event: None,
            next: None,
            prev: None,
        };
        let head_handle = private::EventHandle(events.push(dummy));

        EVENT_LOOP.with(move |maybe_event_loop| {
            let event_loop = EventLoop {
                _last_runnable_state: false,
                events: RefCell::new(events),
                head: head_handle,
                tail: Cell::new(head_handle),
                depth_first_insertion_point: Cell::new(head_handle), // insert after this node
                currently_firing: Cell::new(None),
                to_destroy: Cell::new(None),
            };

            assert!(maybe_event_loop.borrow().is_none());
            *maybe_event_loop.borrow_mut() = Some(event_loop);
        });

        let wait_scope = WaitScope(::std::marker::PhantomData);
        let result = main(&wait_scope);

        EVENT_LOOP.with(move |maybe_event_loop| {
            let el = ::std::mem::replace(&mut *maybe_event_loop.borrow_mut(), None);
            match el {
                None => unreachable!(),
                Some(event_loop) => {
                    // If there is still an event other than the head event, then there must have
                    // been a memory leak.
                    let remaining_events = event_loop.events.borrow().len();
                    if remaining_events > 1 {
                        ::std::mem::forget(event_loop); // Prevent double panic.
                        panic!("{} leaked events found when cleaning up event loop. \
                               Perhaps there is a reference cycle containing promises?",
                               remaining_events - 1)
                    }
                }
            }
        });

        result
    }

    fn arm_depth_first(&self, event_handle: private::EventHandle) {
        let insertion_node_next = self.events.borrow()[self.depth_first_insertion_point.get().0]
                                      .next;

        match insertion_node_next {
            Some(next_handle) => {
                self.events.borrow_mut()[next_handle.0].prev = Some(event_handle);
                self.events.borrow_mut()[event_handle.0].next = Some(next_handle);
            }
            None => {
                self.tail.set(event_handle);
            }
        }

        self.events.borrow_mut()[event_handle.0].prev = Some(self.depth_first_insertion_point
                                                                 .get());
        self.events.borrow_mut()[self.depth_first_insertion_point.get().0].next =
            Some(event_handle);
        self.depth_first_insertion_point.set(event_handle);
    }

    fn arm_breadth_first(&self, event_handle: private::EventHandle) {
        let events = &mut *self.events.borrow_mut();
        events[self.tail.get().0].next = Some(event_handle);
        events[event_handle.0].prev = Some(self.tail.get());
        self.tail.set(event_handle);
    }

    /// Runs the event loop for a single step.
    fn turn(&self) -> bool {

        let event_handle = match self.events.borrow()[self.head.0].next {
            None => return false,
            Some(event_handle) => event_handle,
        };
        self.depth_first_insertion_point.set(event_handle);

        self.currently_firing.set(Some(event_handle));
        let mut event = ::std::mem::replace(&mut self.events.borrow_mut()[event_handle.0].event,
                                            None)
                            .expect("No event to fire?");
        event.fire();
        self.currently_firing.set(None);

        let maybe_next = self.events.borrow()[event_handle.0].next;
        self.events.borrow_mut()[self.head.0].next = maybe_next;
        if let Some(e) = maybe_next {
            self.events.borrow_mut()[e.0].prev = Some(self.head);
        }

        self.events.borrow_mut()[event_handle.0].next = None;
        self.events.borrow_mut()[event_handle.0].prev = None;

        if self.tail.get() == event_handle {
            self.tail.set(self.head);
        }

        self.depth_first_insertion_point.set(self.head);

        if let Some(event_handle) = self.to_destroy.get() {
            self.events.borrow_mut().remove(event_handle.0);
            self.to_destroy.set(None);
        }

        true
    }
}

/// Specifies an error to generate when a [`PromiseFulfiller`](struct.PromiseFulfiller.html) is
/// dropped.
pub trait FulfillerDropped {
    fn fulfiller_dropped() -> Self;
}

/// A handle that can be used to fulfill or reject a promise. If you think of a promise
/// as the receiving end of a oneshot channel, then this is the sending end.
///
/// When a `PromiseFulfiller<T,E>` is dropped without first receiving a `fulfill()`, `reject()`, or
/// `resolve()` call, its promise is rejected with the error value `E::fulfiller_dropped()`.
pub struct PromiseFulfiller<T, E>
    where T: 'static,
          E: 'static + FulfillerDropped
{
    hub: Rc<RefCell<private::PromiseAndFulfillerHub<T, E>>>,
    done: bool,
}

impl<T, E> PromiseFulfiller<T, E>
    where T: 'static,
          E: 'static + FulfillerDropped
{
    pub fn fulfill(mut self, value: T) {
        self.hub.borrow_mut().resolve(Ok(value));
        self.done = true;
    }

    pub fn reject(mut self, error: E) {
        self.hub.borrow_mut().resolve(Err(error));
        self.done = true;
    }

    pub fn resolve(mut self, result: Result<T, E>) {
        self.hub.borrow_mut().resolve(result);
        self.done = true;
    }
}

impl<T, E> Drop for PromiseFulfiller<T, E>
    where T: 'static,
          E: 'static + FulfillerDropped
{
    fn drop(&mut self) {
        if !self.done {
            self.hub.borrow_mut().resolve(Err(E::fulfiller_dropped()));
        }
    }
}

impl FulfillerDropped for () {
    fn fulfiller_dropped() -> () {
        ()
    }
}

impl FulfillerDropped for ::std::io::Error {
    fn fulfiller_dropped() -> ::std::io::Error {
        ::std::io::Error::new(::std::io::ErrorKind::Other,
                              "Promise fulfiller was dropped.")
    }
}

/// Holds a collection of `Promise<T, E>`s and ensures that each executes to completion.
/// Destroying a `TaskSet` automatically cancels all of its unfinished promises.
pub struct TaskSet<T, E>
    where T: 'static,
          E: 'static
{
    task_set_impl: private::TaskSetImpl<T, E>,
}

impl<T, E> TaskSet<T, E> {
    pub fn new(reaper: Box<TaskReaper<T, E>>) -> TaskSet<T, E> {
        TaskSet { task_set_impl: private::TaskSetImpl::new(reaper) }
    }

    pub fn add(&mut self, promise: Promise<T, E>) {
        self.task_set_impl.add(promise.node);
    }
}

/// Callbacks to be invoked when a task in a [`TaskSet`](struct.TaskSet.html) finishes. You are
/// required to implement at least the failure case.
pub trait TaskReaper<T, E>
    where T: 'static,
          E: 'static
{
    fn task_succeeded(&mut self, _value: T) {}
    fn task_failed(&mut self, error: E);
}
