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

use std::rc::Rc;
use std::cell::RefCell;
use {Result, Error, Promise};
use private::{Event, EventDropper, EventHandle, OnReadyEvent, PromiseNode};


/// A PromiseNode that transforms the result of another PromiseNode through an application-provided
/// function (implements `then()`).
pub struct Transform<T, DepT, Func, ErrorFunc>
where Func: FnOnce(DepT) -> Result<T>, ErrorFunc: FnOnce(Error) -> Result<T> {
    dependency: Box<PromiseNode<DepT>>,
    func: Func,
    error_handler: ErrorFunc,
}

impl <T, DepT, Func, ErrorFunc> Transform<T, DepT, Func, ErrorFunc>
where Func: FnOnce(DepT) -> Result<T>, ErrorFunc: FnOnce(Error) -> Result<T> {
    pub fn new(dependency: Box<PromiseNode<DepT>>, func: Func, error_handler: ErrorFunc)
           -> Transform<T, DepT, Func, ErrorFunc> {
        Transform { dependency : dependency,
                    func: func, error_handler: error_handler }
    }
}

impl <T, DepT, Func, ErrorFunc> PromiseNode<T> for Transform<T, DepT, Func, ErrorFunc>
where Func: FnOnce(DepT) -> Result<T>, ErrorFunc: FnOnce(Error) -> Result<T> {
    fn on_ready(&mut self, event: EventHandle) {
        self.dependency.on_ready(event);
    }
    fn get(self: Box<Self>) -> Result<T> {
        let tmp = *self;
        let Transform {dependency, func, error_handler} = tmp;
        match dependency.get() {
            Ok(value) => {
                func(value)
            }
            Err(e) => {
                error_handler(e)
            }
        }
    }
}

/// A promise that has already been resolved to an immediate value or error.
pub struct Immediate<T> {
    result: Result<T>,
}

impl <T> Immediate<T> {
    pub fn new(result: Result<T>) -> Immediate<T> {
        Immediate { result: result }
    }
}

impl <T> PromiseNode<T> for Immediate<T> {
    fn on_ready(&mut self, event: EventHandle) {
        event.arm_breadth_first();
    }
    fn get(self: Box<Self>) -> Result<T> {
        self.result
    }
}

enum ChainState<T> {
    Step1(Box<PromiseNode<Promise<T>>>, Option<EventHandle>),
    Step2(Box<PromiseNode<T>>, Option<EventHandle>),
    Step3 // done
}

struct ChainEvent<T> {
    state: Rc<RefCell<ChainState<T>>>,
}

impl <T> Event for ChainEvent<T> where T: 'static {
    fn fire(&mut self) -> Option<EventDropper> {
        let state = ::std::mem::replace(&mut *self.state.borrow_mut(), ChainState::Step3);
        match state {
            ChainState::Step1(inner, on_ready_event) => {
                match inner.get() {
                    Ok(mut intermediate) => {
                        match on_ready_event {
                            Some(event) => {
                                intermediate.node.on_ready(event);
                            }
                            None => {}
                        }

                        *self.state.borrow_mut() = ChainState::Step2(intermediate.node, None);
                    }
                    Err(e) => {
                        let mut node = Immediate::new(Err(e));
                        match on_ready_event {
                            Some(event) => {
                                node.on_ready(event);
                            }
                            None => {}
                        }

                        *self.state.borrow_mut() = ChainState::Step2(Box::new(node), None);
                    }
                }
            }
            _ => panic!("should be in step 1"),
        }
        return None;
    }
}

/// Promise node that reduces Promise<Promise<T>> to Promise<T>.
pub struct Chain<T> {
    state: Rc<RefCell<ChainState<T>>>,
    dropper: EventDropper,
}

impl <T> Chain<T> where T: 'static {
    pub fn new(mut inner: Box<PromiseNode<Promise<T>>>) -> Chain<T> {

        let state = Rc::new(RefCell::new(ChainState::Step3));
        let event = Box::new(ChainEvent { state: state.clone() });
        let (handle, dropper) = EventHandle::new();
        handle.set(event);
        inner.on_ready(handle);
        *state.borrow_mut() = ChainState::Step1(inner, None);

        Chain { state: state, dropper: dropper }
    }
}

impl <T> PromiseNode<T> for Chain<T> {
    fn on_ready(&mut self, event: EventHandle) {
        match &mut *self.state.borrow_mut() {
            &mut ChainState::Step2(ref mut inner, _) => {
                inner.on_ready(event);
            }
            &mut ChainState::Step1(_, Some(_)) => {
                panic!("on_ready() can only be called once.");
            }
            &mut ChainState::Step1(_, ref mut on_ready_event) => {
                *on_ready_event = Some(event);
            }
            _ => { panic!() }
        }
    }
    fn get(self: Box<Self>) -> Result<T> {
        let state = ::std::mem::replace(&mut *self.state.borrow_mut(), ChainState::Step3);
        match state {
            ChainState::Step2(inner, _) => {
                inner.get()
            }
            _ => {
                panic!()
            }
        }
    }
}


struct ArrayJoinBranch {
    state: Rc<RefCell<ArrayJoinState>>
}

impl Event for ArrayJoinBranch {
    fn fire(&mut self) -> Option<EventDropper> {
        let state = &mut *self.state.borrow_mut();
        state.count_left -= 1;
        if state.count_left == 0 {
            state.on_ready_event.arm();
        }
        return None;
    }
}

struct ArrayJoinState {
    count_left: usize,
    on_ready_event: OnReadyEvent,
}

pub struct ArrayJoin<T> {
    state: Rc<RefCell<ArrayJoinState>>,
    branches: Vec<(Box<PromiseNode<T>>, EventDropper)>,
}

impl<T> ArrayJoin<T> {
    pub fn new(nodes: Vec<Box<PromiseNode<T>>>) -> ArrayJoin<T> {
        let state = Rc::new(RefCell::new(ArrayJoinState { count_left: nodes.len(),
                                                          on_ready_event: OnReadyEvent::Empty }));
        let branches =
            nodes.into_iter()
            .map(|mut node| {
                let (handle, dropper) = EventHandle::new();
                node.on_ready(handle);
                handle.set(Box::new(ArrayJoinBranch { state: state.clone()}));
                return (node, dropper);
            }).collect();
        return ArrayJoin {state: state, branches: branches};
    }
}

impl <T> PromiseNode<Vec<T>> for ArrayJoin<T> {
    fn on_ready(&mut self, event: EventHandle) {
        self.state.borrow_mut().on_ready_event.init(event);
    }
    fn get(self: Box<Self>) -> Result<Vec<T>> {
        let mut result = Vec::new();
        for (dependency, _dropper) in self.branches {
            result.push(try!(dependency.get()));
        }
        return Ok(result);
    }
}

enum ExclusiveJoinSide { Left, Right }

struct ExclusiveJoinBranch<T> {
    state: Rc<RefCell<ExclusiveJoinState<T>>>,
    side: ExclusiveJoinSide,
}

impl<T> Event for ExclusiveJoinBranch<T> {
    fn fire(&mut self) -> Option<EventDropper> {
        let state = &mut *self.state.borrow_mut();
        match self.side {
            ExclusiveJoinSide::Left => {
                state.right = None
            }
            ExclusiveJoinSide::Right => {
                state.left = None
            }
        }
        state.on_ready_event.arm();
        return None;
    }
}

struct ExclusiveJoinState<T> {
    on_ready_event: OnReadyEvent,
    left: Option<(Box<PromiseNode<T>>, EventDropper)>,
    right: Option<(Box<PromiseNode<T>>, EventDropper)>,

}

pub struct ExclusiveJoin<T> where T: 'static {
    state: Rc<RefCell<ExclusiveJoinState<T>>>,
}

impl<T> ExclusiveJoin<T> {
    pub fn new(mut left: Box<PromiseNode<T>>, mut right: Box<PromiseNode<T>>) -> ExclusiveJoin<T> {

        let state = Rc::new(RefCell::new(ExclusiveJoinState {
            on_ready_event: OnReadyEvent::Empty,
            left: None, right: None}));

        {
            let (handle, dropper) = EventHandle::new();
            left.on_ready(handle);
            handle.set(Box::new(ExclusiveJoinBranch { state: state.clone(),
                                                      side: ExclusiveJoinSide::Left }));

            state.borrow_mut().left = Some((left, dropper))
        }

        {
            let (handle, dropper) = EventHandle::new();
            right.on_ready(handle);
            handle.set(Box::new(ExclusiveJoinBranch { state: state.clone(),
                                                      side: ExclusiveJoinSide::Right }));

            state.borrow_mut().right = Some((right, dropper))
        }

        return ExclusiveJoin { state: state };
    }
}

impl <T> PromiseNode<T> for ExclusiveJoin<T> {
    fn on_ready(&mut self, event: EventHandle) {
        self.state.borrow_mut().on_ready_event.init(event);
    }
    fn get(self: Box<Self>) -> Result<T> {
        let left = ::std::mem::replace(&mut self.state.borrow_mut().left, None);
        let right = ::std::mem::replace(&mut self.state.borrow_mut().right, None);

        match (left, right) {
            (Some((node, _dropper)), None) => {
                return node.get();
            }
            (None, Some((node, _dropper))) => {
                return node.get();
            }
            _ => {
                unreachable!()
            }
        }
    }
}

pub struct Wrapper<T, U> where T: 'static {
    node: Box<PromiseNode<T>>,
    inner: U,
}

impl <T, U> Wrapper<T, U> {
    pub fn new(node: Box<PromiseNode<T>>, inner: U) -> Wrapper<T, U> {
        Wrapper { node: node, inner: inner }
    }
}

impl <T, U> PromiseNode<T> for Wrapper<T, U> {
    fn on_ready(&mut self, event: EventHandle) {
        self.node.on_ready(event);
    }
    fn get(self: Box<Self>) -> Result<T> {
        self.node.get()
    }
}

