#![allow(dead_code)]

use std::cell::RefCell;

thread_local!(static EVENT_LOOP: RefCell<Option<EventLoop>> = RefCell::new(None));

fn with_current_event_loop<F, R>(f: F) -> R
    where F: FnOnce(&mut EventLoop) -> R {
        EVENT_LOOP.with(|maybe_event_loop| {
            match &mut *maybe_event_loop.borrow_mut() {
                &mut None => panic!("current thread has no event loop"),
                &mut Some(ref mut event_loop) => f(event_loop),
            }
        })
    }

pub type Error = Box<::std::error::Error>;
pub type Result<T> = ::std::result::Result<T, Error>;

pub struct Promise<T> where T: 'static {
    pub node : Box<PromiseNode<T>>,
}

impl <T> Promise <T> where T: 'static {
    pub fn then<F, G, R>(self, func: F, error_handler: G) -> Promise<R>
        where F: 'static + FnOnce(T) -> Result<R>,
              G: 'static + FnOnce(Error) -> Result<R>,
              R: 'static {
            let node : Box<PromiseNode<R>> = Box::new(TransformPromiseNode::new(self.node, func, error_handler));
            Promise { node: node }
        }

    pub fn wait(mut self) -> Result<T> {
        with_current_event_loop(move |event_loop| {
            let fired = ::std::rc::Rc::new(::std::cell::Cell::new(false));
            let done_event = BoolEvent { fired: fired.clone()};
            self.node.on_ready(Box::new(done_event));

            event_loop.running = true;

            while !fired.get() {
                if !event_loop.turn() {
                    // No events in the queue.
                    panic!("need to implement EventPort");
                }
            }

            self.node.get()
        })
    }
}

pub trait Event {
    fn fire(&mut self);


    /* TODO why doesn't this worked? Something about Sized?
    fn arm_breadth_first(self: Box<Self>) {
        with_current_event_loop(|event_loop| {
     //       event_loop.borrow_mut().arm_breadth_first(self);
        });
    } */
}

struct BoolEvent {
    fired: ::std::rc::Rc<::std::cell::Cell<bool>>,
}

impl Event for BoolEvent {
    fn fire(&mut self) {
        self.fired.set(true);
    }
}

pub trait PromiseNode<T> {
    /// Arms the given event when the promised value is ready.
    fn on_ready(&mut self, event : Box<Event>);

    fn set_self_pointer(&mut self) {}
    fn get(self: Box<Self>) -> Result<T>;
}

/// A PromiseNode that transforms the result of another PromiseNode through an application-provided
/// function (implements `then()`).
pub struct TransformPromiseNode<T, DepT, Func, ErrorFunc>
where Func: FnOnce(DepT) -> Result<T>, ErrorFunc: FnOnce(Error) -> Result<T> {
    dependency: Box<PromiseNode<DepT>>,
    func: Func,
    error_handler: ErrorFunc,
}

impl <T, DepT, Func, ErrorFunc> TransformPromiseNode<T, DepT, Func, ErrorFunc>
where Func: FnOnce(DepT) -> Result<T>, ErrorFunc: FnOnce(Error) -> Result<T> {
    fn new(dependency: Box<PromiseNode<DepT>>, func: Func, error_handler: ErrorFunc)
           -> TransformPromiseNode<T, DepT, Func, ErrorFunc> {
        TransformPromiseNode { dependency : dependency,
                               func: func, error_handler: error_handler }
    }
}

impl <T, DepT, Func, ErrorFunc> PromiseNode<T> for TransformPromiseNode<T, DepT, Func, ErrorFunc>
where Func: FnOnce(DepT) -> Result<T>, ErrorFunc: FnOnce(Error) -> Result<T> {
    fn on_ready(&mut self, event: Box<Event>) {
        self.dependency.on_ready(event);
    }
    fn get(self: Box<Self>) -> Result<T> {
        let tmp = *self;
        let TransformPromiseNode {dependency, func, error_handler} = tmp;
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
pub struct ImmediatePromiseNode<T> {
    result: Result<T>,
}

impl <T> PromiseNode<T> for ImmediatePromiseNode<T> {

    fn on_ready(&mut self, event: Box<Event>) {
        with_current_event_loop(|event_loop| {
            event_loop.arm_breadth_first(event);
        });
    }
    fn get(self: Box<Self>) -> Result<T> {
        self.result
    }
}

trait ErrorHandler {
    fn task_failed(error: Error);
}

struct TaskSetImpl {
    error_handler: Box<ErrorHandler>,
}

/// A queue of events being executed in a loop.
pub struct EventLoop {
//    daemons: TaskSetImpl,
    running: bool,
    last_runnable_state: bool,
    events: ::std::collections::VecDeque<Box<Event>>,
    depth_first_events: ::std::collections::VecDeque<Box<Event>>,
}

impl EventLoop {
    fn init() {
        EVENT_LOOP.with(|maybe_event_loop| {
            let event_loop = EventLoop { running: false,
                                         last_runnable_state: false,
                                         events: ::std::collections::VecDeque::new(),
                                         depth_first_events: ::std::collections::VecDeque::new() };

            *maybe_event_loop.borrow_mut() = Some(event_loop);
        });
    }

    fn arm_depth_first(&mut self, event: Box<Event>) {
        self.depth_first_events.push_front(event);
    }

    fn arm_breadth_first(&mut self, event: Box<Event>) {
        self.events.push_back(event);
    }

    /// Run the event loop for `max_turn_count` turns or until there is nothing left to be done,
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

    fn turn(&mut self) -> bool {
        assert!(self.depth_first_events.is_empty());
        match self.events.pop_front() {
            None => return false,
            Some(mut event) => {
                // event->firing = true ?
                event.fire();
            }
        }
        while !self.depth_first_events.is_empty() {
            self.events.push_front(self.depth_first_events.pop_back().unwrap());
        }
        return true;
    }
}

/// A callback which can be used to fulfill a promise.
pub trait PromiseFulfiller<T> where T: 'static {
    fn fulfill(&mut self, value: T);
    fn reject(&mut self, error: Error);
}

pub enum OnReadyEvent {
    Empty,
    AlreadyReady,
    Full(Box<Event>),
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

    fn init(&mut self, new_event: Box<Event>) {
        if self.is_already_ready() {
            with_current_event_loop(|event_loop| {
                event_loop.arm_breadth_first(new_event);
            });
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
                    with_current_event_loop(|event_loop| {
                        event_loop.arm_depth_first(event);
                    });
                }
                _ => {
                    panic!("armed an event twice?");
                }
            }
        }
    }
}

pub struct PromiseAndFulfillerHub<T> where T: 'static {
    result: Option<Result<T>>,
    on_ready_event: OnReadyEvent,
}

impl <T> PromiseFulfiller<T> for PromiseAndFulfillerHub<T> where T: 'static {
    fn fulfill(&mut self, value: T) {
        if self.result.is_none() {
            self.result = Some(Ok(value));
        }
        self.on_ready_event.arm();
    }

    fn reject(&mut self, error: Error) {
        if self.result.is_none() {
            self.result = Some(Err(error));
        }
    }
}

/*
pub struct WrapperPromiseNode<T> where T: 'static {
    hub: ::std::rc::Rc<::std::cell::RefCell<PromiseAndFulfillerHub<T>>>,
}
*/

impl <T> PromiseNode<T> for ::std::rc::Rc<::std::cell::RefCell<PromiseAndFulfillerHub<T>>> where T: 'static {
    fn on_ready(&mut self, event: Box<Event>) {
        self.borrow_mut().on_ready_event.init(event);
    }
    fn get(self: Box<Self>) -> Result<T> {
        match ::std::mem::replace(&mut self.borrow_mut().result, None) {
            None => panic!("no result!"),
            Some(r) => r
        }
    }
}

impl <T> PromiseFulfiller<T> for ::std::rc::Rc<::std::cell::RefCell<PromiseAndFulfillerHub<T>>> where T: 'static {
    fn fulfill(&mut self, value: T) {
        self.borrow_mut().fulfill(value);
    }

    fn reject(&mut self, error: Error) {
        self.borrow_mut().reject(error);
    }
}

pub fn new_promise_and_fulfiller<T>() -> (Promise<T>, Box<PromiseFulfiller<T>>) where T: 'static {
    let result = ::std::rc::Rc::new(::std::cell::RefCell::new(
        PromiseAndFulfillerHub { result: None::<Result<T>>, on_ready_event: OnReadyEvent::Empty }));
    let result_promise : Promise<T> = Promise { node: Box::new(result.clone())};
    (result_promise, Box::new(result))
}

#[cfg(test)]
mod tests {

    #[test]
    fn hello() {
        ::EventLoop::init();
        let (mut promise, mut fulfiller) = ::new_promise_and_fulfiller::<u32>();
        let p1 = promise.then(|x| {
            println!("x = {}", x);
            return Ok(x + 1);
        }, |e| {
            return Ok(1);
        });

        fulfiller.fulfill(10);
        p1.wait();

    }
}
