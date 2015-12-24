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

use gj::Promise;

#[test]
fn eval_void() {
    use std::rc::Rc;
    use std::cell::Cell;
    gj::EventLoop::top_level(|wait_scope| {
        let done = Rc::new(Cell::new(false));
        let done1 = done.clone();
        let promise: gj::Promise<(), ()> =
            gj::Promise::ok(()).map(move |()| {
                done1.clone().set(true);
                Ok(())
            });
        assert_eq!(done.get(), false);
        promise.wait(wait_scope).unwrap();
        assert_eq!(done.get(), true);
        Ok(())
    }).unwrap();
}

#[test]
fn eval_int() {
    gj::EventLoop::top_level(|wait_scope| {
        let promise: gj::Promise<u64, ()> =
            gj::Promise::ok(19u64).map(|x| {
                assert_eq!(x, 19);
                Ok(x + 2)
            });
        let value = promise.wait(wait_scope).unwrap();
        assert_eq!(value, 21);
        Ok(())
    }).unwrap();
}


#[test]
fn fulfiller() {
    gj::EventLoop::top_level(|wait_scope| {
        let (promise, fulfiller) = Promise::<u32, ()>::and_fulfiller();
        let p1 = promise.map(|x| {
            assert_eq!(x, 10);
            return Ok(x + 1);
        });

        fulfiller.fulfill(10);
        let value = p1.wait(wait_scope).unwrap();
        assert_eq!(value, 11);
        Ok(())
    }).unwrap();
}

#[test]
fn reject_fulfiller() {
    gj::EventLoop::top_level(|wait_scope| {
        let (promise, fulfiller) = Promise::<(), ()>::and_fulfiller();
        fulfiller.reject(());
        let value = promise.wait(wait_scope);
        assert_eq!(value, Err(()));
        Ok(())
    }).unwrap();
}

#[test]
fn drop_fulfiller() {
    gj::EventLoop::top_level(|wait_scope| {
        let (promise, _) = Promise::<(), ()>::and_fulfiller();
        let value = promise.wait(wait_scope);
        assert_eq!(value, Err(()));
        Ok(())
    }).unwrap();
}

#[test]
fn chain() {
    gj::EventLoop::top_level(|wait_scope| {

        let promise: gj::Promise<i32, ()> = gj::Promise::ok(()).map(|()| { return Ok(123); });
        let promise2: gj::Promise<i32, ()> = gj::Promise::ok(()).map(|()| { return Ok(321); });

        let promise3 = promise.then(move |i| {
            promise2.then(move |j| {
                gj::Promise::ok(i + j)
            })
        });

        let value = promise3.wait(wait_scope).unwrap();
        assert_eq!(444, value);
        Ok(())
    }).unwrap();
}

#[test]
fn chain_error() {
    gj::EventLoop::top_level(|wait_scope| {

        let promise = gj::Promise::ok(()).map(|()| { Ok("123") });
        let promise2: gj::Promise<&'static str, Box<::std::error::Error>> =
            gj::Promise::ok(()).map(|()| { Ok("XXX321") });

        let promise3 = promise.then(move |istr| {
            promise2.then(move |jstr| {
                let i: i32 = pry!(istr.parse());
                let j: i32 = pry!(jstr.parse());  // Should return an error.
                gj::Promise::ok(i + j)
            })
        });

        assert!(promise3.wait(wait_scope).is_err());
        Ok(())
    }).unwrap();
}

#[test]
fn deep_chain2() {
    gj::EventLoop::top_level(|wait_scope| {

        let mut promise: gj::Promise<u32, ()> = gj::Promise::ok(4u32);

        for _ in 0..1000 {
            promise = gj::Promise::ok(()).then(|_| {
                promise
            });
        }

        let value = promise.wait(wait_scope).unwrap();

        assert_eq!(value, 4);
        Ok(())
    }).unwrap();
}

#[test]
fn separate_fulfiller_chained() {
    gj::EventLoop::top_level(|wait_scope| {

        let (promise, fulfiller) = Promise::<gj::Promise<i32, ()>, ()>::and_fulfiller();
        let (inner_promise, inner_fulfiller) = Promise::<i32, ()>::and_fulfiller();

        fulfiller.fulfill(inner_promise);
        inner_fulfiller.fulfill(123);

        let value = promise.wait(wait_scope).unwrap()
            .wait(wait_scope).unwrap(); // KJ gets away with only one wait() here.
        assert_eq!(value, 123);
        Ok(())
    }).unwrap();
}

#[test]
fn ordering() {
    use std::rc::Rc;
    use std::cell::{Cell, RefCell};

    gj::EventLoop::top_level(|wait_scope| {

        let counter = Rc::new(Cell::new(0u32));
        let (counter0, counter1, counter2, counter3, counter4, counter5, counter6) =
            (counter.clone(), counter.clone(), counter.clone(), counter.clone(), counter.clone(),
             counter.clone(), counter.clone());

        let mut promises: Vec<Rc<RefCell<Option<gj::Promise<(), ()>>>>> = Vec::new();
        for _ in 0..6 {
            promises.push(Rc::new(RefCell::new(None)));
        }

        let promise2 = promises[2].clone();
        let promise3 = promises[3].clone();
        let promise4 = promises[4].clone();
        let promise5 = promises[5].clone();
        *promises[1].borrow_mut() = Some(gj::Promise::ok(()).then(move |_| {
            assert_eq!(counter0.get(), 0);
            counter0.set(1);

            {
                // Use a promise and fulfiller so that we can fulfill the promise after waiting on it in
                // order to induce depth-first scheduling.
                let (promise, fulfiller) = Promise::<(), ()>::and_fulfiller();
                *promise2.borrow_mut() = Some(promise.then(move |_| {
                    assert_eq!(counter1.get(), 1);
                    counter1.set(2);
                    gj::Promise::ok(())
                }));
                fulfiller.fulfill(());
            }

            // .map() is scheduled breadth-first if the promise has already resolved, but depth-first
            // if the promise resolves later.
            *promise3.borrow_mut() = Some(gj::Promise::ok(()).then(move |_| {
                assert_eq!(counter4.get(), 4); // XXX
                counter4.set(5);
                gj::Promise::ok(())
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
                    gj::Promise::ok(())
                }));
                fulfiller.fulfill(());
            }

            *promise5.borrow_mut() = Some(gj::Promise::ok(()).map(move |_| {
                assert_eq!(counter6.get(), 6);
                counter6.set(7);
                Ok(())
            }));

            gj::Promise::ok(())
        }));

        *promises[0].borrow_mut() = Some(gj::Promise::ok(()).then(move |_| {
            assert_eq!(counter3.get(), 3);
            counter3.set(4);
            gj::Promise::ok(())
        }));

        for p in promises.into_iter() {
            let maybe_p = ::std::mem::replace(&mut *p.borrow_mut(), None);
            match maybe_p {
                None => {}
                Some(p) => {
                    p.wait(wait_scope).unwrap()
                }
            }
        }

        assert_eq!(counter.get(), 7);
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
fn task_set() {
    gj::EventLoop::top_level(|wait_scope| {
        let reaper = TaskReaperImpl::new();
        let error_count = reaper.get_error_count();
        let mut tasks = gj::TaskSet::new(Box::new(reaper));
        tasks.add(gj::Promise::ok(()).map(|()| {
            Ok(())
        }));
        tasks.add(gj::Promise::ok(()).map(|()| {
            Err(())
        }));
        tasks.add(gj::Promise::ok(()).map(|()| {
            Ok(())
        }));

        gj::Promise::ok(()).then(|()| -> gj::Promise<(), ()> {
            panic!("Promise without waiter shouldn't execute.");
        });

        gj::Promise::ok(()).map(|()| -> Result<(), ()> {
            panic!("Promise without waiter shouldn't execute.");
        });

        gj::Promise::<(), ()>::ok(()).map(|()| { Ok(()) } ).wait(wait_scope).unwrap();

        assert_eq!(error_count.get(), 1);
        Ok(())
    }).unwrap();
}

#[test]
fn drop_task_set() {
    gj::EventLoop::top_level(|wait_scope| {
        let mut tasks = gj::TaskSet::new(Box::new(TaskReaperImpl::new()));
        let panicker = gj::Promise::ok(()).map(|()| { unreachable!() });
        tasks.add(panicker);
        drop(tasks);
        gj::Promise::<(), ()>::ok(()).map(|()| { Ok(()) } ).wait(wait_scope).unwrap();
        Ok(())
    }).unwrap();
}

#[test]
fn drop_task_set_leak() {
    // At one point, this causes a "leaked events" panic.
    gj::EventLoop::top_level(|_wait_scope| {
        let mut tasks = gj::TaskSet::new(Box::new(TaskReaperImpl::new()));
        tasks.add(gj::Promise::never_done());
        Ok(())
    }).unwrap();
}

#[test]
fn array_join() {
    gj::EventLoop::top_level(|wait_scope| {
        let promises: Vec<gj::Promise<u32, ()>> =
            vec![gj::Promise::ok(123),
                 gj::Promise::ok(456),
                 gj::Promise::ok(789)];

        let promise = Promise::all(promises.into_iter());
        let result = promise.wait(wait_scope).unwrap();

        assert_eq!(result.len(), 3);
        assert_eq!(result[0], 123);
        assert_eq!(result[1], 456);
        assert_eq!(result[2], 789);
        Ok(())
    }).unwrap();
}

#[test]
fn array_join_drop_then_fulfill() {
    gj::EventLoop::top_level(|wait_scope| {
        let (p, fulfiller) = Promise::<(), ()>::and_fulfiller();
        let p1: Promise<(),()> = p.map(|()| { panic!("this should never execute.") });
        let promises = vec![p1];
        let promise = Promise::all(promises.into_iter()).eagerly_evaluate();
        drop(promise);
        fulfiller.fulfill(());

        // Get the event loop turning.
        let wait_promise: gj::Promise<(),()> = gj::Promise::ok(()).then(|()| Promise::ok(()));
        wait_promise.wait(wait_scope).unwrap();

        Ok(())
    }).unwrap();
}

#[test]
fn exclusive_join() {
    gj::EventLoop::top_level(|wait_scope| {
        let left = gj::Promise::ok(()).map(|()| {
            return Ok(123);
        });
        let (right, _fulfiller) = Promise::<u32, ()>::and_fulfiller();
        let result = left.exclusive_join(right).wait(wait_scope).unwrap();

        assert_eq!(result, 123);
        Ok(())
    }).unwrap();

    gj::EventLoop::top_level(|wait_scope| {
        let (left, _fulfiller) = Promise::<u32, ()>::and_fulfiller();
        let right = gj::Promise::ok(()).map(|()| {
            return Ok(456);
        });

        let result = left.exclusive_join(right).wait(wait_scope).unwrap();

        assert_eq!(result, 456);
        Ok(())
    }).unwrap();

    gj::EventLoop::top_level(|wait_scope| {
        let left: gj::Promise<u32, ()> = gj::Promise::ok(()).map(|()| {
            Ok(123)
        });
        let right: gj::Promise<u32, ()> = gj::Promise::ok(()).map(|()| {
            Ok(456)
        }); // need to eagerly evaluate?

        let _result = left.exclusive_join(right).wait(wait_scope).unwrap();

//        assert_eq!(result, 456);
        Ok(())
    }).unwrap();
}


#[test]
fn simple_recursion() {
    fn foo(n: u64) -> gj::Promise<(), ()> {
        gj::Promise::ok(()).then(move |()| {
            if n == 0 {
                gj::Promise::ok(())
            } else {
                foo(n-1)
            }
        })
    }

    gj::EventLoop::top_level(|wait_scope| {
        Ok(foo(100000).wait(wait_scope).unwrap())
    }).unwrap();
}

#[test]
fn task_set_recursion() {
    // At one point, this causes a "leaked events" panic.

    fn foo(n: u64, maybe_fulfiller: Option<gj::PromiseFulfiller<(),()>>) -> gj::Promise<(), ()> {
        gj::Promise::ok(()).then(move |()| {
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

    gj::EventLoop::top_level(|wait_scope| {
        let mut tasks = gj::TaskSet::new(Box::new(TaskReaperImpl::new()));
        let (promise, fulfiller) = Promise::and_fulfiller();
        tasks.add(foo(2, Some(fulfiller)));
        promise.wait(wait_scope).unwrap();
        Ok(())
    }).unwrap();
}

#[test]
fn fork() {
    gj::EventLoop::top_level(|wait_scope| {
        let promise: gj::Promise<i32, ()> = gj::Promise::ok(()).map(|()| { Ok(123) });
        let mut fork = promise.fork();
        let branch1 = fork.add_branch().map(|i| {
            assert_eq!(123, i);
            Ok(456)
        });
        let branch2 = fork.add_branch().map(|i| {
            assert_eq!(123, i);
            Ok(789)
        });
        drop(fork);

        assert_eq!(456, branch1.wait(wait_scope).unwrap());
        assert_eq!(789, branch2.wait(wait_scope).unwrap());
        Ok(())
    }).unwrap();
}

#[test]
#[should_panic(expected = "Promise callback destroyed itself.")]
#[allow(path_statements)]
fn knotty() {
    use std::rc::Rc;
    use std::cell::RefCell;
    gj::EventLoop::top_level(|wait_scope| {
        let maybe_promise = Rc::new(RefCell::new(None));
        let maybe_promise2 = maybe_promise.clone();

        let knotted_promise: gj::Promise<(), ()> = gj::Promise::ok(()).map(move |()| {

            // We will arrange for this to be the only remaining reference to knotted_promise.
            // AFter the callback runs, the promise will get dropped, triggering a panic.
            maybe_promise2;

            Ok(())
        }).eagerly_evaluate();

        *maybe_promise.borrow_mut() = Some(knotted_promise);
        drop(maybe_promise);

        // Get the event loop turning.
        let wait_promise: gj::Promise<(),()> = gj::Promise::ok(());
        wait_promise.wait(wait_scope).unwrap();
        Ok(())
    }).unwrap()
}

#[test]
#[allow(unused_assignments)]
fn eagerly_evaluate() {
    use std::rc::Rc;
    use std::cell::Cell;

    gj::EventLoop::top_level(|wait_scope| {
        let called: Rc<Cell<bool>> = Rc::new(Cell::new(false));
        let called1 = called.clone();
        let mut promise: gj::Promise<(),()> = gj::Promise::ok(()).map(move |()| {
            called1.set(true);
            Ok(())
        });
        gj::Promise::<(),()>::ok(()).map(|()|{Ok(())}).wait(wait_scope).unwrap();
        assert_eq!(false, called.get());

        promise = promise.eagerly_evaluate();
        gj::Promise::<(),()>::ok(()).map(|()|{Ok(())}).wait(wait_scope).unwrap();

        assert_eq!(true, called.get());
        Ok(())
    }).unwrap();
}
