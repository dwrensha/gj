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

extern crate gj;

#[test]
fn eval_void() {
    use std::rc::Rc;
    use std::cell::Cell;
    gj::EventLoop::top_level(|wait_scope| {
        let done = Rc::new(Cell::new(false));
        let done1 = done.clone();
        let promise: gj::Promise<(), ()> =
            gj::Promise::fulfilled(()).map(move |()| {
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
            gj::Promise::fulfilled(19u64).map(|x| {
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
        let (promise, fulfiller) = gj::new_promise_and_fulfiller::<u32, ()>();
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
        let (promise, fulfiller) = gj::new_promise_and_fulfiller::<(), ()>();
        fulfiller.reject(());
        let value = promise.wait(wait_scope);
        assert_eq!(value, Err(()));
        Ok(())
    }).unwrap();
}

#[test]
fn drop_fulfiller() {
    gj::EventLoop::top_level(|wait_scope| {
        let (promise, _) = gj::new_promise_and_fulfiller::<(), ()>();
        let value = promise.wait(wait_scope);
        assert_eq!(value, Err(()));
        Ok(())
    }).unwrap();
}

#[test]
fn chain() {
    gj::EventLoop::top_level(|wait_scope| {

        let promise: gj::Promise<i32, ()> = gj::Promise::fulfilled(()).map(|()| { return Ok(123); });
        let promise2: gj::Promise<i32, ()> = gj::Promise::fulfilled(()).map(|()| { return Ok(321); });

        let promise3 = promise.then(move |i| {
            return Ok(promise2.then(move |j| {
                return Ok(gj::Promise::fulfilled(i + j));
            }));
        });

        let value = promise3.wait(wait_scope).unwrap();
        assert_eq!(444, value);
        Ok(())
    }).unwrap();
}

#[test]
fn chain_error() {
    gj::EventLoop::top_level(|wait_scope| {

        let promise = gj::Promise::fulfilled(()).map(|()| { Ok("123") });
        let promise2: gj::Promise<&'static str, Box<::std::error::Error>> =
            gj::Promise::fulfilled(()).map(|()| { Ok("XXX321") });

        let promise3 = promise.then(move |istr| {
            Ok(promise2.then(move |jstr| {
                let i: i32 = try!(istr.parse());
                let j: i32 = try!(jstr.parse());  // Should return an error.
                Ok(gj::Promise::fulfilled(i + j))
            }))
        });

        assert!(promise3.wait(wait_scope).is_err());
        Ok(())
    }).unwrap();
}

#[test]
fn deep_chain2() {
    gj::EventLoop::top_level(|wait_scope| {

        let mut promise: gj::Promise<u32, ()> = gj::Promise::fulfilled(4u32);

        for _ in 0..1000 {
            promise = gj::Promise::fulfilled(()).then(|_| {
                Ok(promise)
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

        let (promise, fulfiller) = gj::new_promise_and_fulfiller::<gj::Promise<i32, ()>, ()>();
        let (inner_promise, inner_fulfiller) = gj::new_promise_and_fulfiller::<i32, ()>();

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
        *promises[1].borrow_mut() = Some(gj::Promise::fulfilled(()).then(move |_| {
            assert_eq!(counter0.get(), 0);
            counter0.set(1);

            {
                // Use a promise and fulfiller so that we can fulfill the promise after waiting on it in
                // order to induce depth-first scheduling.
                let (promise, fulfiller) = gj::new_promise_and_fulfiller::<(), ()>();
                *promise2.borrow_mut() = Some(promise.then(move |_| {
                    assert_eq!(counter1.get(), 1);
                    counter1.set(2);
                    return Ok(gj::Promise::fulfilled(()));
                }));
                fulfiller.fulfill(());
            }

            // .map() is scheduled breadth-first if the promise has already resolved, but depth-first
            // if the promise resolves later.
            *promise3.borrow_mut() = Some(gj::Promise::fulfilled(()).then(move |_| {
                assert_eq!(counter4.get(), 4); // XXX
                counter4.set(5);
                return Ok(gj::Promise::fulfilled(()));
            }).map(move |_| {
                assert_eq!(counter5.get(), 5);
                counter5.set(6);
                return Ok(());
            }));

            {
                let (promise, fulfiller) = gj::new_promise_and_fulfiller::<(), ()>();
                *promise4.borrow_mut() = Some(promise.then(move |_| {
                    assert_eq!(counter2.get(), 2);
                    counter2.set(3);
                    return Ok(gj::Promise::fulfilled(()));
                }));
                fulfiller.fulfill(());
            }

            *promise5.borrow_mut() = Some(gj::Promise::fulfilled(()).map(move |_| {
                assert_eq!(counter6.get(), 6);
                counter6.set(7);
                return Ok(());
            }));

            return Ok(gj::Promise::fulfilled(()));
        }));

        *promises[0].borrow_mut() = Some(gj::Promise::fulfilled(()).then(move |_| {
            assert_eq!(counter3.get(), 3);
            counter3.set(4);
            return Ok(gj::Promise::fulfilled(()));
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

impl gj::TaskReaper<(), Box<::std::error::Error>> for TaskReaperImpl {
    fn task_failed(&mut self, _error: Box<::std::error::Error>) {
        self.error_count.set(self.error_count.get() + 1);
    }
}

#[test]
fn task_set() {
    gj::EventLoop::top_level(|wait_scope| {
        let error_count = ::std::rc::Rc::new(::std::cell::Cell::new(0));
        let mut tasks = gj::TaskSet::new(Box::new(TaskReaperImpl {error_count: error_count.clone()}));
        tasks.add(gj::Promise::fulfilled(()).map(|()| {
            Ok(())
        }));
        tasks.add(gj::Promise::fulfilled(()).map(|()| {
            Err(::std::io::Error::new(::std::io::ErrorKind::Other, "Fake IO Error"))
        }).lift());
        tasks.add(gj::Promise::fulfilled(()).map(|()| {
            Ok(())
        }));

        gj::Promise::fulfilled(()).then(|()| -> Result<gj::Promise<(), ()>, ()> {
            panic!("Promise without waiter shouldn't execute.");
        });

        gj::Promise::fulfilled(()).map(|()| -> Result<(), ()> {
            panic!("Promise without waiter shouldn't execute.");
        });

        gj::Promise::<(), ()>::fulfilled(()).map(|()| { Ok(()) } ).wait(wait_scope).unwrap();

        assert_eq!(error_count.get(), 1);
        Ok(())
    }).unwrap();
}

#[test]
fn array_join() {
    gj::EventLoop::top_level(|wait_scope| {
        let promises: Vec<gj::Promise<u32, ()>> =
            vec![gj::Promise::fulfilled(123),
                 gj::Promise::fulfilled(456),
                 gj::Promise::fulfilled(789)];

        let promise = gj::join_promises(promises);
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
    gj::EventLoop::top_level(|_wait_scope| {
        let (p, fulfiller) = gj::new_promise_and_fulfiller::<(), ()>();
        let promises = vec![p];
        let promise = gj::join_promises(promises);
        drop(promise);
        fulfiller.fulfill(());
        Ok(())
    }).unwrap();
}

#[test]
fn exclusive_join() {
    gj::EventLoop::top_level(|wait_scope| {
        let left = gj::Promise::fulfilled(()).map(|()| {
            return Ok(123);
        });
        let (right, _fulfiller) = gj::new_promise_and_fulfiller::<u32, ()>();
        let result = left.exclusive_join(right).wait(wait_scope).unwrap();

        assert_eq!(result, 123);
        Ok(())
    }).unwrap();

    gj::EventLoop::top_level(|wait_scope| {
        let (left, _fulfiller) = gj::new_promise_and_fulfiller::<u32, ()>();
        let right = gj::Promise::fulfilled(()).map(|()| {
            return Ok(456);
        });

        let result = left.exclusive_join(right).wait(wait_scope).unwrap();

        assert_eq!(result, 456);
        Ok(())
    }).unwrap();

    gj::EventLoop::top_level(|wait_scope| {
        let left: gj::Promise<u32, ()> = gj::Promise::fulfilled(()).map(|()| {
            Ok(123)
        });
        let right: gj::Promise<u32, ()> = gj::Promise::fulfilled(()).map(|()| {
            Ok(456)
        }); // need to eagerly evaluate?

        let _result = left.exclusive_join(right).wait(wait_scope).unwrap();

//        assert_eq!(result, 456);
        Ok(())
    }).unwrap();
}


#[test]
fn recursion() {
    fn foo(n: u64) -> gj::Promise<(), ()> {
        gj::Promise::fulfilled(()).then(move |()| {
            if n == 0 {
                Ok(gj::Promise::fulfilled(()))
            } else {
                Ok(foo(n-1))
            }
        })
    }

    gj::EventLoop::top_level(|wait_scope| {
        Ok(foo(100000).wait(wait_scope).unwrap())
    }).unwrap();
}

#[test]
fn fork() {
    gj::EventLoop::top_level(|wait_scope| {
        let promise: gj::Promise<i32, ()> = gj::Promise::fulfilled(()).map(|()| { Ok(123) });
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

        let knotted_promise: gj::Promise<(), ()> = gj::Promise::fulfilled(()).map(move |()| {

            // We will arrange for this to be the only remaining reference to knotted_promise.
            // AFter the callback runs, the promise will get dropped, triggering a panic.
            maybe_promise2;

            Ok(())
        }).eagerly_evaluate();

        *maybe_promise.borrow_mut() = Some(knotted_promise);
        drop(maybe_promise);

        // Get the event loop turning.
        let wait_promise: gj::Promise<(),()> = gj::Promise::fulfilled(());
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
        let mut promise: gj::Promise<(),()> = gj::Promise::fulfilled(()).map(move |()| {
            called1.set(true);
            Ok(())
        });
        gj::Promise::<(),()>::fulfilled(()).map(|()|{Ok(())}).wait(wait_scope).unwrap();
        assert_eq!(false, called.get());

        promise = promise.eagerly_evaluate();
        gj::Promise::<(),()>::fulfilled(()).map(|()|{Ok(())}).wait(wait_scope).unwrap();

        assert_eq!(true, called.get());
        Ok(())
    }).unwrap();
}
