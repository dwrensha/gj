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
use std::result::Result;
use {Promise};
use private::{Event, EventDropper, OpaqueEventDropper, EventHandle, OnReadyEvent, PromiseNode};


/// A PromiseNode that transforms the result of another PromiseNode through an application-provided
/// function (implements `map()`).
pub struct Transform<T, E, E1, DepT, Func>
where Func: FnOnce(Result<DepT, E>) -> Result<T, E1> {
    dependency: Box<PromiseNode<DepT, E>>,
    func: Func,
}

impl <T, E, E1, DepT, Func> Transform<T, E, E1, DepT, Func>
    where Func: FnOnce(Result<DepT, E>) -> Result<T, E1>
{
    pub fn new(dependency: Box<PromiseNode<DepT, E>>, func: Func) -> Transform<T, E, E1, DepT, Func> {
        Transform { dependency: dependency, func: func}
    }
}

impl <T, E, E1, DepT, Func> PromiseNode<T, E1> for Transform<T, E, E1, DepT, Func>
    where Func: FnOnce(Result<DepT, E>) -> Result<T, E1>
{
    fn on_ready(&mut self, event: EventHandle) {
        self.dependency.on_ready(event);
    }
    fn get(self: Box<Self>) -> Result<T, E1> {
        let tmp = *self;
        let Transform {dependency, func} = tmp;
        func(dependency.get())
    }
}

/// A promise that has already been resolved to an immediate value or error.
pub struct Immediate<T, E> {
    result: Result<T, E>,
}

impl <T, E> Immediate<T, E> {
    pub fn new(result: Result<T, E>) -> Immediate<T, E> {
        Immediate { result: result }
    }
}

impl <T, E> PromiseNode<T, E> for Immediate<T, E> {
    fn on_ready(&mut self, event: EventHandle) {
        event.arm_breadth_first();
    }
    fn get(self: Box<Self>) -> Result<T, E> {
        self.result
    }
}

/// A promise that never resolves.
pub struct NeverDone<T, E> {
    phantom_data: ::std::marker::PhantomData<Result<T, E>>,
}

impl <T, E> NeverDone<T, E> {
    pub fn new() -> NeverDone<T, E> {
        NeverDone { phantom_data: ::std::marker::PhantomData }
    }
}

impl <T, E> PromiseNode<T, E> for NeverDone<T, E> {
    fn on_ready(&mut self, _event: EventHandle) {
        // ignore
    }
    fn get(self: Box<Self>) -> Result<T, E> {
        panic!("not ready")
    }
}

pub enum ChainState<T, E> where T: 'static, E: 'static {
    Step1(Box<PromiseNode<Promise<T, E>, E>>, Option<EventHandle>, Option<Rc<RefCell<ChainState<T, E>>>>),
    Step2(Box<PromiseNode<T, E>>, Option<Rc<RefCell<ChainState<T, E>>>>),
    Step3 // done
}

pub struct ChainEvent<T, E> where T: 'static, E: 'static {
    state: Rc<RefCell<ChainState<T, E>>>,
}

impl <T, E> Event for ChainEvent<T, E> {
    fn fire(&mut self) -> Option<Box<OpaqueEventDropper>> {
        let state = ::std::mem::replace(&mut *self.state.borrow_mut(), ChainState::Step3);
        match state {
            ChainState::Step1(inner, on_ready_event, self_ptr) => {
                match inner.get() {
                    Ok(mut intermediate) => {
                        match on_ready_event {
                            Some(event) => {
                                intermediate.node.on_ready(event);
                            }
                            None => {}
                        }

                        *self.state.borrow_mut() = ChainState::Step2(intermediate.node, self_ptr);
                    }
                    Err(e) => {
                        let mut node = Immediate::new(Err(e));
                        match on_ready_event {
                            Some(event) => {
                                node.on_ready(event);
                            }
                            None => {}
                        }

                        *self.state.borrow_mut() = ChainState::Step2(Box::new(node), self_ptr);
                    }
                }

                let mut shorten = false;
                let self_state = self.state.clone(); // TODO better control flow
                match &mut *self.state.borrow_mut() {
                    &mut ChainState::Step2(_, Some(_)) => {
                        shorten = true;
                    }
                    &mut ChainState::Step2(ref mut inner, None) => {
                        inner.set_self_pointer(self_state);
                    }
                    _ => { unreachable!() }
                }

                if shorten {
                    let state = ::std::mem::replace(&mut *self.state.borrow_mut(), ChainState::Step3);

                    match state {
                        ChainState::Step2(mut inner, Some(self_ptr)) => {
                            inner.set_self_pointer(self_ptr.clone());
                            let self_state = ::std::mem::replace(&mut *self_ptr.borrow_mut(),
                                                                 ChainState::Step2(inner, None));
                            match self_state {
                                ChainState::Step2(inner, _) => {
                                    Some(Box::new(inner))
                                }
                                _ => unreachable!(),
                            }
                        }
                        _ => { unreachable!() }
                    }
                } else {
                    None
                }
            }
            _ => panic!("should be in step 1"),
        }
    }
}

/// Promise node that reduces Promise<Promise<T>> to Promise<T>.
pub struct Chain<T, E> where T: 'static, E: 'static {
    state: Rc<RefCell<ChainState<T, E>>>,
    dropper: EventDropper,
}

impl <T, E> Chain<T, E> {
    pub fn new(mut inner: Box<PromiseNode<Promise<T, E>, E>>) -> Chain<T, E> {

        let state = Rc::new(RefCell::new(ChainState::Step3));
        let event = Box::new(ChainEvent { state: state.clone() });
        let (handle, dropper) = EventHandle::new();
        handle.set(event);
        inner.on_ready(handle);
        *state.borrow_mut() = ChainState::Step1(inner, None, None);

        Chain { state: state, dropper: dropper }
    }
}

impl <T, E> PromiseNode<T, E> for Chain<T, E> {
    fn on_ready(&mut self, event: EventHandle) {
        match &mut *self.state.borrow_mut() {
            &mut ChainState::Step2(ref mut inner, _) => {
                inner.on_ready(event);
            }
            &mut ChainState::Step1(_, Some(_), _) => {
                panic!("on_ready() can only be called once.");
            }
            &mut ChainState::Step1(_, ref mut on_ready_event, _) => {
                *on_ready_event = Some(event);
            }
            _ => { panic!() }
        }
    }
    fn get(self: Box<Self>) -> Result<T, E> {
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
    fn set_self_pointer(&mut self, chain_state: Rc<RefCell<ChainState<T, E>>>) {
        match &mut *self.state.borrow_mut() {
            &mut ChainState::Step1(_, _, ref mut self_ptr) => {
                *self_ptr = Some(chain_state);
            }
            &mut ChainState::Step2(_, ref mut self_ptr) => {
                *self_ptr = Some(chain_state);
            }
            _ => { panic!() }
        }
    }
}


struct ArrayJoinBranch {
    state: Rc<RefCell<ArrayJoinState>>
}

impl Event for ArrayJoinBranch {
    fn fire(&mut self) -> Option<Box<OpaqueEventDropper>> {
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

pub struct ArrayJoin<T, E> {
    state: Rc<RefCell<ArrayJoinState>>,
    branches: Vec<(Box<PromiseNode<T, E>>, EventDropper)>,
}

impl<T, E> ArrayJoin<T, E> {
    pub fn new(nodes: Vec<Box<PromiseNode<T, E>>>) -> ArrayJoin<T, E> {
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

impl <T, E> PromiseNode<Vec<T>, E> for ArrayJoin<T, E> {
    fn on_ready(&mut self, event: EventHandle) {
        self.state.borrow_mut().on_ready_event.init(event);
    }
    fn get(self: Box<Self>) -> Result<Vec<T>, E> {
        let mut result = Vec::new();
        for (dependency, _dropper) in self.branches {
            result.push(try!(dependency.get()));
        }
        return Ok(result);
    }
}

enum ExclusiveJoinSide { Left, Right }

struct ExclusiveJoinBranch<T, E> {
    state: Rc<RefCell<ExclusiveJoinState<T, E>>>,
    side: ExclusiveJoinSide,
}

impl<T, E> Event for ExclusiveJoinBranch<T, E> {
    fn fire(&mut self) -> Option<Box<OpaqueEventDropper>> {
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

struct ExclusiveJoinState<T, E> {
    on_ready_event: OnReadyEvent,
    left: Option<(Box<PromiseNode<T, E>>, EventDropper)>,
    right: Option<(Box<PromiseNode<T, E>>, EventDropper)>,
}

pub struct ExclusiveJoin<T, E> where T: 'static, E: 'static {
    state: Rc<RefCell<ExclusiveJoinState<T, E>>>,
}

impl<T, E> ExclusiveJoin<T, E> {
    pub fn new(mut left: Box<PromiseNode<T, E>>, mut right: Box<PromiseNode<T, E>>) -> ExclusiveJoin<T, E> {

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

impl <T, E> PromiseNode<T, E> for ExclusiveJoin<T, E> {
    fn on_ready(&mut self, event: EventHandle) {
        self.state.borrow_mut().on_ready_event.init(event);
    }
    fn get(self: Box<Self>) -> Result<T, E> {
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

pub struct Wrapper<T, U, E> where T: 'static, E: 'static {
    node: Box<PromiseNode<T, E>>,
    inner: U,
}

impl <T, U, E> Wrapper<T, U, E> {
    pub fn new(node: Box<PromiseNode<T, E>>, inner: U) -> Wrapper<T, U, E> {
        Wrapper { node: node, inner: inner }
    }
}

impl <T, U, E> PromiseNode<T, E> for Wrapper<T, U, E> {
    fn on_ready(&mut self, event: EventHandle) {
        self.node.on_ready(event);
    }
    fn get(self: Box<Self>) -> Result<T, E> {
        self.node.get()
    }
}

