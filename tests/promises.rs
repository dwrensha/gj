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

#[macro_use]
extern crate gj;

use gj::{EventLoop, Promise, ClosedEventPort};

#[test]
fn eval_void() {
    use std::rc::Rc;
    use std::cell::Cell;
    EventLoop::top_level(|wait_scope| -> Result<(), ()> {
        let done = Rc::new(Cell::new(false));
        let done1 = done.clone();
        let promise: Promise<(), ()> =
            Promise::ok(()).map(move |()| {
                done1.clone().set(true);
                Ok(())
            });
        assert_eq!(done.get(), false);
        try!(promise.wait(wait_scope, &mut ClosedEventPort(())));
        assert_eq!(done.get(), true);
        Ok(())
    }).unwrap();
}

#[test]
fn eval_int() {
    EventLoop::top_level(|wait_scope| -> Result<(), ()> {
        let promise: Promise<u64, ()> =
            Promise::ok(19u64).map(|x| {
                assert_eq!(x, 19);
                Ok(x + 2)
            });
        let value = try!(promise.wait(wait_scope, &mut ClosedEventPort(())));
        assert_eq!(value, 21);
        Ok(())
    }).unwrap();
}

#[test]
fn fulfiller() {
    EventLoop::top_level(|wait_scope| -> Result<(), ()> {
        let (promise, fulfiller) = Promise::<u32, ()>::and_fulfiller();
        let p1 = promise.map(|x| {
            assert_eq!(x, 10);
            Ok(x + 1)
        });

        fulfiller.fulfill(10);
        let value = try!(p1.wait(wait_scope, &mut ClosedEventPort(())));
        assert_eq!(value, 11);
        Ok(())
    }).unwrap();
}

#[derive(PartialEq, Eq, Clone, Debug)]
enum FulfillerError { Dropped, Rejected, NeverFulfilled }
impl gj::FulfillerDropped for FulfillerError {
    fn fulfiller_dropped() -> FulfillerError { FulfillerError::Dropped }
}

#[test]
fn reject_fulfiller() {
    EventLoop::top_level(|wait_scope| -> Result<(),()> {
        let (promise, fulfiller) = Promise::<(), FulfillerError>::and_fulfiller();
        fulfiller.reject(FulfillerError::Rejected);
        let value = promise.wait(wait_scope, &mut ClosedEventPort(FulfillerError::NeverFulfilled));
        assert_eq!(value, Err(FulfillerError::Rejected));
        Ok(())
    }).unwrap();
}

#[test]
fn drop_fulfiller() {
    EventLoop::top_level(|wait_scope| -> Result<(),()> {
        let (promise, _) = Promise::<(), FulfillerError>::and_fulfiller();
        let value = promise.wait(wait_scope, &mut ClosedEventPort(FulfillerError::NeverFulfilled));
        assert_eq!(value, Err(FulfillerError::Dropped));
        Ok(())
    }).unwrap();
}

#[test]
fn hold_fulfiller() {
    EventLoop::top_level(|wait_scope| -> Result<(), ()> {
        let (promise, _fulfiller) = Promise::<(),FulfillerError>::and_fulfiller();
        let value = promise.wait(wait_scope, &mut ClosedEventPort(FulfillerError::NeverFulfilled));
        assert_eq!(value, Err(FulfillerError::NeverFulfilled));
        Ok(())
    }).unwrap();
}

#[test]
fn chain() {
    EventLoop::top_level(|wait_scope| -> Result<(), ()> {
        let promise: Promise<i32, ()> = Promise::ok(()).map(|()| { Ok(123) });
        let promise2: Promise<i32, ()> = Promise::ok(()).map(|()| { Ok(321) });

        let promise3 = promise.then(move |i| {
            promise2.then(move |j| {
                Promise::ok(i + j)
            })
        });

        let value = try!(promise3.wait(wait_scope, &mut ClosedEventPort(())));
        assert_eq!(444, value);
        Ok(())
    }).unwrap();
}

#[test]
fn chain_error() {
    EventLoop::top_level(|wait_scope| -> Result<(),()> {
        let promise = Promise::ok(()).map(|()| { Ok("123") });
        let promise2: Promise<&'static str, Box<::std::error::Error>> =
            Promise::ok(()).map(|()| { Ok("XXX321") });

        let promise3 = promise.then(move |istr| {
            promise2.then(move |jstr| {
                let i: i32 = pry!(istr.parse());
                let j: i32 = pry!(jstr.parse());  // Should return an error.
                Promise::ok(i + j)
            })
        });

        assert!(promise3.wait(wait_scope, &mut ClosedEventPort("EVENT PORT CLOSED")).is_err());
        Ok(())
    }).unwrap();
}

#[test]
fn deep_chain2() {
    EventLoop::top_level(|wait_scope| -> Result<(),()> {
        let mut promise: Promise<u32, ()> = Promise::ok(4u32);

        for _ in 0..1000 {
            promise = Promise::ok(()).then(|_| {
                promise
            });
        }

        let value = try!(promise.wait(wait_scope, &mut ClosedEventPort(())));

        assert_eq!(value, 4);
        Ok(())
    }).unwrap();
}

#[test]
fn separate_fulfiller_chained() {
    EventLoop::top_level(|wait_scope| -> Result<(),()> {
        let mut event_port = ClosedEventPort(());
        let (promise, fulfiller) = Promise::<Promise<i32, ()>, ()>::and_fulfiller();
        let (inner_promise, inner_fulfiller) = Promise::<i32, ()>::and_fulfiller();

        fulfiller.fulfill(inner_promise);
        inner_fulfiller.fulfill(123);

        let value = try!(try!(promise.wait(wait_scope, &mut event_port))
            .wait(wait_scope, &mut event_port)); // KJ gets away with only one wait() here.
        assert_eq!(value, 123);
        Ok(())
    }).unwrap();
}

#[test]
fn ordering() {
    use std::rc::Rc;
    use std::cell::{Cell, RefCell};

    EventLoop::top_level(|wait_scope| -> Result<(),()> {
        let mut event_port = ClosedEventPort(());

        let counter = Rc::new(Cell::new(0u32));
        let (counter0, counter1, counter2, counter3, counter4, counter5, counter6) =
            (counter.clone(), counter.clone(), counter.clone(), counter.clone(), counter.clone(),
             counter.clone(), counter.clone());

        let mut promises: Vec<Rc<RefCell<Option<Promise<(), ()>>>>> = Vec::new();
        for _ in 0..6 {
            promises.push(Rc::new(RefCell::new(None)));
        }

        let promise2 = promises[2].clone();
        let promise3 = promises[3].clone();
        let promise4 = promises[4].clone();
        let promise5 = promises[5].clone();
        *promises[1].borrow_mut() = Some(Promise::ok(()).then(move |_| {
            assert_eq!(counter0.get(), 0);
            counter0.set(1);

            {
                // Use a promise and fulfiller so that we can fulfill the promise after waiting on it in
                // order to induce depth-first scheduling.
                let (promise, fulfiller) = Promise::<(), ()>::and_fulfiller();
                *promise2.borrow_mut() = Some(promise.then(move |_| {
                    assert_eq!(counter1.get(), 1);
                    counter1.set(2);
                    Promise::ok(())
                }));
                fulfiller.fulfill(());
            }

            // .map() is scheduled breadth-first if the promise has already resolved, but depth-first
            // if the promise resolves later.
            *promise3.borrow_mut() = Some(Promise::ok(()).then(move |_| {
                assert_eq!(counter4.get(), 4); // XXX
                counter4.set(5);
                Promise::ok(())
            }).map(move |_| {
                assert_eq!(counter5.get(), 5);
                counter5.set(6);
                Ok(())
            }));

            {
                let (promise, fulfiller) = Promise::<(), ()>::and_fulfiller();
                *promise4.borrow_mut() = Some(promise.then(move |_| {
                    assert_eq!(counter2.get(), 2);
                    counter2.set(3);
                    Promise::ok(())
                }));
                fulfiller.fulfill(());
            }

            *promise5.borrow_mut() = Some(Promise::ok(()).map(move |_| {
                assert_eq!(counter6.get(), 6);
                counter6.set(7);
                Ok(())
            }));

            Promise::ok(())
        }));

        *promises[0].borrow_mut() = Some(Promise::ok(()).then(move |_| {
            assert_eq!(counter3.get(), 3);
            counter3.set(4);
            Promise::ok(())
        }));

        for p in promises.into_iter() {
            let maybe_p = ::std::mem::replace(&mut *p.borrow_mut(), None);
            match maybe_p {
                None => {}
                Some(p) => {
                    try!(p.wait(wait_scope, &mut event_port))
                }
            }
        }

        assert_eq!(counter.get(), 7);
        Ok(())
    }).unwrap();
}

#[test]
fn drop_depth_first_insertion_point() {
    // At one point, this triggered an "invalid handle idx" panic.
    EventLoop::top_level(|_wait_scope| -> Result<(),()> {
        let (promise, fulfiller) = Promise::<(), ()>::and_fulfiller();
        let promise = Promise::all(vec![promise].into_iter());
        drop(fulfiller);
        drop(promise);

        let (promise1, fulfiller1) = Promise::<(), ()>::and_fulfiller();
        let promise1 = Promise::all(vec![promise1].into_iter());
        drop(fulfiller1);
        drop(promise1);

        Ok(())
    }).unwrap();
}

#[test]
fn drop_tail() {
    // At one point, this failed to terminate.
    EventLoop::top_level(|wait_scope| -> Result<(),()> {
        let mut event_port = ClosedEventPort(());
        let (promise, fulfiller) = Promise::<(), ()>::and_fulfiller();

        let mut fork = promise.fork();
        let branch1 = fork.add_branch();
        let branch2 = fork.add_branch();

        let promise1 = Promise::<(),()>::ok(()).then(|()| {
            Promise::ok(())
        }).then(|()| {
            Promise::ok(())
        });

        fulfiller.reject(());

        drop(fork);
        drop(branch1);
        drop(branch2);

        try!(promise1.wait(wait_scope, &mut event_port));
        Ok(())
    }).unwrap();
}


pub struct TaskReaperImpl {
    error_count: ::std::rc::Rc<::std::cell::Cell<u32>>,
}

impl TaskReaperImpl {
    fn new() -> TaskReaperImpl {
        TaskReaperImpl { error_count: ::std::rc::Rc::new(::std::cell::Cell::new(0)) }
    }
    fn get_error_count(&self) -> ::std::rc::Rc<::std::cell::Cell<u32>> {
        self.error_count.clone()
    }
}

impl gj::TaskReaper<(), ()> for TaskReaperImpl {
    fn task_failed(&mut self, _error: ()) {
        self.error_count.set(self.error_count.get() + 1);
    }
}

#[test]
#[allow(unused_must_use)]
fn task_set() {
    EventLoop::top_level(|wait_scope| -> Result<(),()> {
        let mut event_port = ClosedEventPort(());
        let reaper = TaskReaperImpl::new();
        let error_count = reaper.get_error_count();
        let mut tasks = gj::TaskSet::new(Box::new(reaper));
        tasks.add(Promise::ok(()).map(|()| {
            Ok(())
        }));
        tasks.add(Promise::ok(()).map(|()| {
            Err(())
        }));
        tasks.add(Promise::ok(()).map(|()| {
            Ok(())
        }));

        gj::Promise::ok(()).then(|()| -> Promise<(), ()> {
            panic!("Promise without waiter shouldn't execute.");
        });

        gj::Promise::ok(()).map(|()| -> Result<(), ()> {
            panic!("Promise without waiter shouldn't execute.");
        });

        try!(gj::Promise::<(), ()>::ok(()).map(|()| Ok(())).wait(wait_scope, &mut event_port));

        assert_eq!(error_count.get(), 1);
        Ok(())
    }).unwrap();
}

#[test]
fn drop_task_set() {
    gj::EventLoop::top_level(|wait_scope| -> Result<(),()> {
        let mut event_port = ClosedEventPort(());
        let mut tasks = gj::TaskSet::new(Box::new(TaskReaperImpl::new()));
        let panicker = Promise::ok(()).map(|()| { unreachable!() });
        tasks.add(panicker);
        drop(tasks);
        try!(Promise::<(), ()>::ok(()).map(|()| Ok(())).wait(wait_scope, &mut event_port));
        Ok(())
    }).unwrap();
}

#[test]
fn drop_task_set_leak() {
    // At one point, this caused a "leaked events" panic.
    EventLoop::top_level(|_wait_scope| -> Result<(),()> {
        let mut tasks = gj::TaskSet::new(Box::new(TaskReaperImpl::new()));
        tasks.add(Promise::never_done());
        Ok(())
    }).unwrap();
}

#[test]
fn array_join_simple() {
    EventLoop::top_level(|wait_scope| -> Result<(),()> {
        let mut event_port = ClosedEventPort(());
        let promises: Vec<Promise<u32, ()>> =
            vec![Promise::ok(123),
                 Promise::ok(456),
                 Promise::ok(789)];

        let promise = Promise::all(promises.into_iter());
        let result = try!(promise.wait(wait_scope, &mut event_port));

        assert_eq!(result.len(), 3);
        assert_eq!(result[0], 123);
        assert_eq!(result[1], 456);
        assert_eq!(result[2], 789);
        Ok(())
    }).unwrap();
}

#[test]
fn array_join_empty() {
    EventLoop::top_level(|wait_scope| -> Result<(),()> {
        let mut event_port = ClosedEventPort(());
        let promises: Vec<Promise<(), ()>> = Vec::new();
        let promise = Promise::all(promises.into_iter());
        let result = try!(promise.wait(wait_scope, &mut event_port));
        assert_eq!(result.len(), 0);
        Ok(())
    }).unwrap();
}

#[test]
fn array_join_ordering() {
    use std::rc::Rc;
    use std::cell::Cell;
    EventLoop::top_level(|wait_scope| -> Result<(),()> {
        let mut event_port = ClosedEventPort(());
        let bad_thing_happened = Rc::new(Cell::new(false));

        let (p1, _f1) = Promise::and_fulfiller();
        let (p2, f2) = Promise::and_fulfiller();
        let (p3, _f3) = Promise::and_fulfiller();

        let bad_thing_happened1 = bad_thing_happened.clone();
        let p1 = p1.map(move |()| {
            bad_thing_happened1.set(true);
            Ok(())
        });

        let bad_thing_happened2 = bad_thing_happened.clone();
        let p2 = p2.map(move |()| {
            bad_thing_happened2.set(true);
            Ok(())
        });

        let bad_thing_happened3 = bad_thing_happened.clone();
        let p3 = p3.map(move |()| {
            bad_thing_happened3.set(true);
            Ok(())
        });

        let promises: Vec<Promise<(), ()>> = vec![p1, p2, p3];

        let promise = Promise::all(promises.into_iter());

        f2.reject(());

        assert_eq!(promise.wait(wait_scope, &mut event_port), Err(()));
        assert_eq!(bad_thing_happened.get(), false);

        Ok(())
    }).unwrap();
}


#[test]
fn array_join_drop_then_fulfill() {
    EventLoop::top_level(|wait_scope| -> Result<(),()> {
        let mut event_port = ClosedEventPort(());
        let (p, fulfiller) = Promise::<(), ()>::and_fulfiller();
        let p1: Promise<(),()> = p.map(|()| { panic!("this should never execute.") });
        let promises = vec![p1];
        let promise = Promise::all(promises.into_iter()).eagerly_evaluate();
        drop(promise);
        fulfiller.fulfill(());

        // Get the event loop turning.
        let wait_promise: Promise<(),()> = Promise::ok(()).then(|()| Promise::ok(()));
        try!(wait_promise.wait(wait_scope, &mut event_port));

        Ok(())
    }).unwrap();
}

#[test]
fn exclusive_join() {
    EventLoop::top_level(|wait_scope| -> Result<(),()> {
        let mut event_port = ClosedEventPort(());

        let left = Promise::ok(()).map(|()| Ok(123));
        let (right, _fulfiller) = Promise::<u32, ()>::and_fulfiller();
        let result = try!(left.exclusive_join(right).wait(wait_scope, &mut event_port));

        assert_eq!(result, 123);
        Ok(())
    }).unwrap();

    EventLoop::top_level(|wait_scope| -> Result<(),()> {
        let mut event_port = ClosedEventPort(());
        let (left, _fulfiller) = Promise::<u32, ()>::and_fulfiller();
        let right = Promise::ok(()).map(|()| Ok(456));

        let result = try!(left.exclusive_join(right).wait(wait_scope, &mut event_port));

        assert_eq!(result, 456);
        Ok(())
    }).unwrap();

    EventLoop::top_level(|wait_scope| -> Result<(), ()> {
        let mut event_port = ClosedEventPort(());
        let left: Promise<u32, ()> = Promise::ok(()).map(|()| {
            Ok(123)
        });
        let right: Promise<u32, ()> = Promise::ok(()).map(|()| {
            Ok(456)
        }); // need to eagerly evaluate?

        let _result = try!(left.exclusive_join(right).wait(wait_scope, &mut event_port));

//        assert_eq!(result, 456);
        Ok(())
    }).unwrap();
}


#[test]
fn simple_recursion() {
    fn foo(n: u64) -> Promise<(), ()> {
        Promise::ok(()).then(move |()| {
            if n == 0 {
                Promise::ok(())
            } else {
                foo(n-1)
            }
        })
    }

    EventLoop::top_level(|wait_scope| -> Result<(), ()> {
        let mut event_port = ClosedEventPort(());
        foo(100000).wait(wait_scope, &mut event_port)
    }).unwrap();
}

#[test]
fn task_set_recursion() {
    // At one point, this causes a "leaked events" panic.

    fn foo(n: u64, maybe_fulfiller: Option<gj::PromiseFulfiller<(),()>>) -> Promise<(), ()> {
        Promise::ok(()).then(move |()| {
            match (n, maybe_fulfiller) {
                (0, _) => panic!("this promise should have been cancelled"),
                (1, Some(fulfiller)) => {
                    fulfiller.fulfill(());
                    foo(n-1, None)
                }
                (n, maybe_fulfiller) => {
                    foo(n-1, maybe_fulfiller)
                }
            }
        })
    }

    EventLoop::top_level(|wait_scope| -> Result<(), ()> {
        let mut event_port = ClosedEventPort(());
        let mut tasks = gj::TaskSet::new(Box::new(TaskReaperImpl::new()));
        let (promise, fulfiller) = Promise::and_fulfiller();
        tasks.add(foo(2, Some(fulfiller)));
        try!(promise.wait(wait_scope, &mut event_port));
        Ok(())
    }).unwrap();
}

#[test]
fn fork_simple() {
    EventLoop::top_level(|wait_scope| -> Result<(),()> {
        let mut event_port = ClosedEventPort(());
        let promise: Promise<i32, ()> = Promise::ok(()).map(|()| { Ok(123) });
        let mut fork = promise.fork();
        let branch1 = fork.add_branch().map(|i| {
            assert_eq!(123, i);
            Ok(456)
        });
        let branch2 = fork.add_branch().map(|i| {
            assert_eq!(123, i);
            Ok(789)
        });
        let branch3 = fork.add_branch().map(|_| {
            Ok(912)
        });
        drop(fork);
        drop(branch3);

        assert_eq!(456, try!(branch1.wait(wait_scope, &mut event_port)));
        assert_eq!(789, try!(branch2.wait(wait_scope, &mut event_port)));
        Ok(())
    }).unwrap();
}

#[test]
fn fork_cancel() {
    use ::std::rc::Rc;
    use ::std::cell::Cell;

    EventLoop::top_level(|wait_scope| -> Result<(),()> {
        let mut event_port = ClosedEventPort(());
        let bad_thing_happened = Rc::new(Cell::new(false));

        let bad_thing_happened1 = bad_thing_happened.clone();
        let promise: Promise<(), ()> = Promise::ok(()).map(move |()| {
            bad_thing_happened1.set(true);
            Ok(())
        });
        let mut fork = promise.fork();
        let branch = fork.add_branch();
        drop(fork);
        drop(branch);

        // There are no references left. The original promise should be cancelled.

        // Get the event loop turning.
        let wait_promise: Promise<(),()> = Promise::ok(()).then(|()| Promise::ok(()));
        try!(wait_promise.wait(wait_scope, &mut event_port));

        assert_eq!(false, bad_thing_happened.get());
        Ok(())
    }).unwrap();
}

#[test]
fn fork_branch_after_resolve() {
    gj::EventLoop::top_level(|wait_scope| -> Result<(),()> {
        let mut event_port = ClosedEventPort(());
        let promise: Promise<(), ()> = Promise::ok(()).then(move |()| {
            Promise::ok(())
        });
        let mut fork = promise.fork();
        let branch = fork.add_branch();

        assert!(branch.wait(wait_scope, &mut event_port).is_ok());

        let branch1 = fork.add_branch();
        assert!(branch1.wait(wait_scope, &mut event_port).is_ok());

        Ok(())
    }).unwrap();
}

#[test]
fn knotty() {
    use std::rc::Rc;
    use std::cell::RefCell;
    EventLoop::top_level(|wait_scope| -> Result<(),()> {
        let mut event_port = ClosedEventPort(());
        let maybe_promise = Rc::new(RefCell::new(None));
        let maybe_promise2 = maybe_promise.clone();

        let knotted_promise: Promise<(), ()> = Promise::ok(()).map(move |()| {
            // We will arrange for this to be the only remaining reference to knotted_promise.
            drop(maybe_promise2);
            Ok(())
        }).eagerly_evaluate();

        *maybe_promise.borrow_mut() = Some(knotted_promise);
        drop(maybe_promise);

        // Get the event loop turning.
        let wait_promise: Promise<(),()> = Promise::ok(());
        try!(wait_promise.wait(wait_scope, &mut event_port));
        Ok(())
    }).unwrap()
}

#[test]
fn task_set_drop_self() {
    // At one point, this panicked with "dangling reference to tasks?"
    use std::rc::Rc;
    use std::cell::RefCell;
    EventLoop::top_level(|wait_scope| -> Result<(),()> {
        let tasks = Rc::new(RefCell::new(gj::TaskSet::new(Box::new(TaskReaperImpl::new()))));

        let (promise, fulfiller) = Promise::<(),()>::and_fulfiller();
        let promise = promise.attach(tasks.clone());
        tasks.borrow_mut().add(promise);

        drop(tasks);
        fulfiller.fulfill(());
        try!(Promise::<(),()>::ok(()).map(|()| Ok(())).wait(wait_scope, &mut ClosedEventPort(())));
        Ok(())
    }).expect("top level");
}

#[test]
#[allow(unused_assignments)]
fn eagerly_evaluate() {
    use std::rc::Rc;
    use std::cell::Cell;

    EventLoop::top_level(|wait_scope| -> Result<(),()> {
        let mut event_port = ClosedEventPort(());
        let called: Rc<Cell<bool>> = Rc::new(Cell::new(false));
        let called1 = called.clone();
        let mut promise: Promise<(),()> = Promise::ok(()).map(move |()| {
            called1.set(true);
            Ok(())
        });
        try!(Promise::<(),()>::ok(()).map(|()| Ok(())).wait(wait_scope, &mut event_port));
        assert_eq!(false, called.get());

        promise = promise.eagerly_evaluate();
        try!(Promise::<(),()>::ok(()).map(|()| Ok(())).wait(wait_scope, &mut event_port));

        assert_eq!(true, called.get());
        Ok(())
    }).unwrap();
}
