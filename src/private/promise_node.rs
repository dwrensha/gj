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

use std::rc::{Rc, Weak};
use std::cell::RefCell;
use std::result::Result;
use Promise;
use private::{Event, EventDropper, GuardedEventHandle, OnReadyEvent, PromiseNode};


/// A PromiseNode that transforms the result of another PromiseNode through an application-provided
/// function (implements `map()`).
pub struct Transform<T, E, E1, DepT, Func>
    where Func: FnOnce(Result<DepT, E>) -> Result<T, E1>
{
    dependency: Box<PromiseNode<DepT, E>>,
    func: Func,
}

impl<T, E, E1, DepT, Func> Transform<T, E, E1, DepT, Func>
    where Func: FnOnce(Result<DepT, E>) -> Result<T, E1>
{
    pub fn new(dependency: Box<PromiseNode<DepT, E>>,
               func: Func)
               -> Transform<T, E, E1, DepT, Func> {
        Transform {
            dependency: dependency,
            func: func,
        }
    }
}

impl<T, E, E1, DepT, Func> PromiseNode<T, E1> for Transform<T, E, E1, DepT, Func>
    where Func: FnOnce(Result<DepT, E>) -> Result<T, E1>
{
    fn on_ready(&mut self, event: GuardedEventHandle) {
        self.dependency.on_ready(event);
    }
    fn get(self: Box<Self>) -> Result<T, E1> {
        let tmp = *self;
        let Transform { dependency, func } = tmp;
        func(dependency.get())
    }
}

/// A promise that has already been resolved to an immediate value or error.
pub struct Immediate<T, E> {
    result: Result<T, E>,
}

impl<T, E> Immediate<T, E> {
    pub fn new(result: Result<T, E>) -> Immediate<T, E> {
        Immediate { result: result }
    }
}

impl<T, E> PromiseNode<T, E> for Immediate<T, E> {
    fn on_ready(&mut self, event: GuardedEventHandle) {
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

impl<T, E> NeverDone<T, E> {
    pub fn new() -> NeverDone<T, E> {
        NeverDone { phantom_data: ::std::marker::PhantomData }
    }
}

impl<T, E> PromiseNode<T, E> for NeverDone<T, E> {
    fn on_ready(&mut self, _event: GuardedEventHandle) {
        // ignore
    }
    fn get(self: Box<Self>) -> Result<T, E> {
        panic!("not ready")
    }
}

pub enum ChainState<T, E>
    where T: 'static,
          E: 'static
{
    Step1(Box<PromiseNode<Promise<T, E>, E>>,
          Option<GuardedEventHandle>,
          Option<Weak<RefCell<ChainState<T, E>>>>),
    Step2(Box<PromiseNode<T, E>>, Option<Weak<RefCell<ChainState<T, E>>>>),
    Step3, // done
}

pub struct ChainEvent<T, E>
    where T: 'static,
          E: 'static
{
    state: Rc<RefCell<ChainState<T, E>>>,
}

impl<T, E> Event for ChainEvent<T, E> {
    fn fire(&mut self) {
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

                let self_state = self.state.clone(); // TODO better control flow
                let maybe_shorten = match *self.state.borrow_mut() {
                    ChainState::Step2(_, Some(ref self_ptr)) => self_ptr.upgrade(),
                    ChainState::Step2(ref mut inner, None) => {
                        inner.set_self_pointer(Rc::downgrade(&self_state));
                        None
                    }
                    _ => unreachable!(),
                };

                if let Some(strong_self_ptr) = maybe_shorten {
                    let state = ::std::mem::replace(&mut *self.state.borrow_mut(),
                                                    ChainState::Step3);

                    match state {
                        ChainState::Step2(mut inner, Some(self_ptr)) => {
                            inner.set_self_pointer(self_ptr.clone());
                            ::std::mem::replace(&mut *strong_self_ptr.borrow_mut(),
                                                ChainState::Step2(inner, None));
                        }
                        _ => unreachable!(),
                    }
                }
            }
            _ => panic!("should be in step 1"),
        }
    }
}

/// Promise node that reduces Promise<Promise<T>> to Promise<T>.
pub struct Chain<T, E>
    where T: 'static,
          E: 'static
{
    state: Rc<RefCell<ChainState<T, E>>>,
    dropper: EventDropper,
}

impl<T, E> Chain<T, E> {
    pub fn new(mut inner: Box<PromiseNode<Promise<T, E>, E>>) -> Chain<T, E> {

        let state = Rc::new(RefCell::new(ChainState::Step3));
        let event = Box::new(ChainEvent { state: state.clone() });
        let (handle, dropper) = GuardedEventHandle::new();
        handle.set(event);
        inner.on_ready(handle);
        *state.borrow_mut() = ChainState::Step1(inner, None, None);

        Chain {
            state: state,
            dropper: dropper,
        }
    }
}

impl<T, E> PromiseNode<T, E> for Chain<T, E> {
    fn on_ready(&mut self, event: GuardedEventHandle) {
        match *self.state.borrow_mut() {
            ChainState::Step2(ref mut inner, _) => {
                inner.on_ready(event);
            }
            ChainState::Step1(_, Some(_), _) => {
                panic!("on_ready() can only be called once.");
            }
            ChainState::Step1(_, ref mut on_ready_event, _) => {
                *on_ready_event = Some(event);
            }
            _ => panic!(),
        }
    }
    fn get(self: Box<Self>) -> Result<T, E> {
        let state = ::std::mem::replace(&mut *self.state.borrow_mut(), ChainState::Step3);
        match state {
            ChainState::Step2(inner, _) => inner.get(),
            _ => panic!(),
        }
    }
    fn set_self_pointer(&mut self, chain_state: Weak<RefCell<ChainState<T, E>>>) {
        match *self.state.borrow_mut() {
            ChainState::Step1(_, _, ref mut self_ptr) => {
                *self_ptr = Some(chain_state);
            }
            ChainState::Step2(_, ref mut self_ptr) => {
                *self_ptr = Some(chain_state);
            }
            _ => panic!(),
        }
    }
}


struct ArrayJoinBranch<T, E>
    where T: 'static,
          E: 'static
{
    index: usize,
    state: Weak<RefCell<ArrayJoinState<T, E>>>,
}

impl<T, E> Event for ArrayJoinBranch<T, E> {
    fn fire(&mut self) {
        let strong_state = self.state.upgrade().expect("dangling pointer?");
        let state = &mut *strong_state.borrow_mut();
        let stage = ::std::mem::replace(&mut state.stage, ArrayJoinStage::Uninit);
        match stage {
            ArrayJoinStage::Uninit => unreachable!(),
            ArrayJoinStage::Cancelled(_) => unreachable!(),
            ArrayJoinStage::Active(mut branches) => {
                let branch_stage = ::std::mem::replace(&mut branches[self.index],
                                                       ArrayBranchStage::Uninit);
                match branch_stage {
                    ArrayBranchStage::Uninit => unreachable!(),
                    ArrayBranchStage::Done(_) => unreachable!(),
                    ArrayBranchStage::Waiting(p, _) => {
                        match p.get() {
                            Ok(v) => {
                                branches[self.index] = ArrayBranchStage::Done(v);
                                state.count_left -= 1;
                                if state.count_left == 0 {
                                    state.on_ready_event.arm();
                                }
                                state.stage = ArrayJoinStage::Active(branches);
                            }
                            Err(e) => {
                                state.stage = ArrayJoinStage::Cancelled(e);
                                state.on_ready_event.arm();
                            }
                        }
                    }
                }
            }
        }
    }
}

enum ArrayBranchStage<T, E>
    where T: 'static,
          E: 'static
{
    Uninit,
    Waiting(Box<PromiseNode<T, E>>, EventDropper),
    Done(T),
}

enum ArrayJoinStage<T, E>
    where T: 'static,
          E: 'static
{
    Uninit,
    Active(Vec<ArrayBranchStage<T, E>>),
    Cancelled(E),
}

struct ArrayJoinState<T, E>
    where T: 'static,
          E: 'static
{
    count_left: usize,
    on_ready_event: OnReadyEvent,
    stage: ArrayJoinStage<T, E>,
}

pub struct ArrayJoin<T, E>
    where T: 'static,
          E: 'static
{
    state: Rc<RefCell<ArrayJoinState<T, E>>>,
}

impl<T, E> ArrayJoin<T, E> {
    pub fn new<I>(promises: I) -> ArrayJoin<T, E>
        where I: Iterator<Item = Promise<T, E>>
    {
        let state = Rc::new(RefCell::new(ArrayJoinState {
            count_left: 0,
            on_ready_event: OnReadyEvent::Empty,
            stage: ArrayJoinStage::Uninit,
        }));
        let mut idx = 0;
        let branches: Vec<ArrayBranchStage<T, E>> =
            promises.into_iter()
                    .map(|promise| {
                        let mut node = promise.node;
                        let (handle, dropper) = GuardedEventHandle::new();
                        node.on_ready(handle.clone());
                        handle.set(Box::new(ArrayJoinBranch {
                            index: idx,
                            state: Rc::downgrade(&state),
                        }));
                        idx += 1;
                        ArrayBranchStage::Waiting(node, dropper)
                    })
                    .collect();
        if branches.len() == 0 {
            state.borrow_mut().on_ready_event.arm();
        }
        state.borrow_mut().count_left = branches.len();
        state.borrow_mut().stage = ArrayJoinStage::Active(branches);
        ArrayJoin { state: state }
    }
}

impl<T, E> PromiseNode<Vec<T>, E> for ArrayJoin<T, E> {
    fn on_ready(&mut self, event: GuardedEventHandle) {
        self.state.borrow_mut().on_ready_event.init(event);
    }
    fn get(self: Box<Self>) -> Result<Vec<T>, E> {
        let stage = ::std::mem::replace(&mut self.state.borrow_mut().stage, ArrayJoinStage::Uninit);
        match stage {
            ArrayJoinStage::Uninit => unreachable!(),
            ArrayJoinStage::Cancelled(e) => Err(e),
            ArrayJoinStage::Active(branches) => {
                let mut result = Vec::new();
                for branch in branches {
                    match branch {
                        ArrayBranchStage::Uninit |
                        ArrayBranchStage::Waiting(..) => unreachable!(),
                        ArrayBranchStage::Done(v) => result.push(v),
                    }
                }
                Ok(result)
            }
        }
    }
}

enum ExclusiveJoinSide {
    Left,
    Right,
}

struct ExclusiveJoinBranch<T, E> {
    state: Rc<RefCell<ExclusiveJoinState<T, E>>>,
    side: ExclusiveJoinSide,
}

impl<T, E> Event for ExclusiveJoinBranch<T, E> {
    fn fire(&mut self) {
        let state = &mut *self.state.borrow_mut();
        match self.side {
            ExclusiveJoinSide::Left => state.right = None,
            ExclusiveJoinSide::Right => state.left = None,
        }
        state.on_ready_event.arm();
    }
}

struct ExclusiveJoinState<T, E> {
    on_ready_event: OnReadyEvent,
    left: Option<(Box<PromiseNode<T, E>>, EventDropper)>,
    right: Option<(Box<PromiseNode<T, E>>, EventDropper)>,
}

pub struct ExclusiveJoin<T, E>
    where T: 'static,
          E: 'static
{
    state: Rc<RefCell<ExclusiveJoinState<T, E>>>,
}

impl<T, E> ExclusiveJoin<T, E> {
    pub fn new(mut left: Box<PromiseNode<T, E>>,
               mut right: Box<PromiseNode<T, E>>)
               -> ExclusiveJoin<T, E> {

        let state = Rc::new(RefCell::new(ExclusiveJoinState {
            on_ready_event: OnReadyEvent::Empty,
            left: None,
            right: None,
        }));

        {
            let (handle, dropper) = GuardedEventHandle::new();
            left.on_ready(handle.clone());
            handle.set(Box::new(ExclusiveJoinBranch {
                state: state.clone(),
                side: ExclusiveJoinSide::Left,
            }));

            state.borrow_mut().left = Some((left, dropper))
        }

        {
            let (handle, dropper) = GuardedEventHandle::new();
            right.on_ready(handle.clone());
            handle.set(Box::new(ExclusiveJoinBranch {
                state: state.clone(),
                side: ExclusiveJoinSide::Right,
            }));

            state.borrow_mut().right = Some((right, dropper))
        }

        return ExclusiveJoin { state: state };
    }
}

impl<T, E> PromiseNode<T, E> for ExclusiveJoin<T, E> {
    fn on_ready(&mut self, event: GuardedEventHandle) {
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
            _ => unreachable!(),
        }
    }
}

enum ForkHubStage<T, E>
    where T: 'static + Clone,
          E: 'static + Clone
{
    Uninitialized,
    Waiting(Box<PromiseNode<T, E>>),
    Done(Result<T, E>),
}

struct ForkHubState<T, E>
    where T: 'static + Clone,
          E: 'static + Clone
{
    branches: Vec<::std::rc::Weak<RefCell<OnReadyEvent>>>,
    stage: ForkHubStage<T, E>,
}

pub struct ForkHub<T, E>
    where T: 'static + Clone,
          E: 'static + Clone
{
    state: Rc<RefCell<ForkHubState<T, E>>>,
    dropper: EventDropper,
}

impl<T, E> ForkHub<T, E>
    where T: 'static + Clone,
          E: 'static + Clone
{
    pub fn new(mut inner: Box<PromiseNode<T, E>>) -> ForkHub<T, E> {
        // TODO(someday): KJ calls setSelfPointer() here.
        let (handle, dropper) = GuardedEventHandle::new();
        inner.on_ready(handle.clone());
        let state = Rc::new(RefCell::new(ForkHubState {
            branches: Vec::new(),
            stage: ForkHubStage::Waiting(inner),
        }));
        let event = Box::new(ForkEvent { state: state.clone() }) as Box<Event>;

        handle.set(event);

        ForkHub {
            state: state,
            dropper: dropper,
        }
    }

    pub fn add_branch(hub: &Rc<RefCell<ForkHub<T, E>>>) -> Promise<T, E> {
        {
            let ref state = &hub.borrow().state;
            match state.borrow().stage {
                ForkHubStage::Done(Ok(ref v)) => {
                    return Promise::ok(v.clone());
                }
                ForkHubStage::Done(Err(ref e)) => {
                    return Promise::err(e.clone());
                }
                _ => {}
            }
            ()
        }

        let on_ready_event = Rc::new(RefCell::new(OnReadyEvent::Empty));
        {
            let state = &mut hub.borrow_mut().state;
            state.borrow_mut().branches.push(Rc::downgrade(&on_ready_event));
        }
        let node = ForkBranch {
            hub: hub.clone(),
            on_ready_event: on_ready_event,
        };
        Promise { node: Box::new(node) }
    }
}

pub struct ForkEvent<T, E>
    where T: 'static + Clone,
          E: 'static + Clone
{
    state: Rc<RefCell<ForkHubState<T, E>>>,
}

impl<T, E> Event for ForkEvent<T, E>
    where T: 'static + Clone,
          E: 'static + Clone
{
    fn fire(&mut self) {
        // Dependency is ready.  Fetch its result and then delete the node.
        let stage = ::std::mem::replace(&mut self.state.borrow_mut().stage,
                                        ForkHubStage::Uninitialized);
        let result = match stage {
            ForkHubStage::Waiting(inner) => inner.get(),
            _ => unreachable!(),
        };

        for branch in &self.state.borrow().branches {
            match branch.upgrade() {
                None => (),
                Some(b) => {
                    b.borrow_mut().arm();
                }
            }
        }
        self.state.borrow_mut().stage = ForkHubStage::Done(result);
    }
}

pub struct ForkBranch<T, E>
    where T: 'static + Clone,
          E: 'static + Clone
{
    hub: Rc<RefCell<ForkHub<T, E>>>,
    on_ready_event: Rc<RefCell<OnReadyEvent>>,
}

impl<T, E> PromiseNode<T, E> for ForkBranch<T, E>
    where T: Clone,
          E: Clone
{
    fn on_ready(&mut self, event: GuardedEventHandle) {
        self.on_ready_event.borrow_mut().init(event);
    }
    fn get(self: Box<Self>) -> Result<T, E> {
        let state = &self.hub.borrow().state;
        let result = match state.borrow().stage {
            ForkHubStage::Done(Ok(ref v)) => Ok(v.clone()),
            ForkHubStage::Done(Err(ref e)) => Err(e.clone()),
            _ => unreachable!(),
        };
        result
    }
}
