#![allow(dead_code)]

use std::cell::RefCell;

thread_local!(static EVENT_LOOP: Option<RefCell<EventLoop>> = None);

fn with_current_event_loop<F, R>(f: F) -> R
    where F: FnOnce(&RefCell<EventLoop>) -> R {
        EVENT_LOOP.with(|maybe_event_loop| {
            match maybe_event_loop {
                &None => panic!("current thread has no event loop"),
                &Some(ref event_loop) => f(event_loop),
            }
        })
    }

pub type Error = Box<::std::error::Error>;
pub type Result<T> = ::std::result::Result<T, Error>;

pub struct Promise<T> {
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

    pub fn wait(self) -> Result<T> {
        unimplemented!()
    }
}

pub trait Event {
    fn fire(&mut self);
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
            event_loop.borrow_mut().arm_breadth_first(event);
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

/// A Deque with an insertion point in the middle.
struct InsertDeque<T> {
    front: ::std::collections::VecDeque<T>,
    back: ::std::collections::VecDeque<T>,
}

/// A queue of events being executed in a loop.
pub struct EventLoop {
    daemons: TaskSetImpl,
    running: bool,
    last_runnable_state: bool,
    events: ::std::collections::VecDeque<Box<Event>>,
    depth_first_events: ::std::collections::VecDeque<Box<Event>>,
}

impl EventLoop {

    fn arm_depth_first(&mut self, event: Box<Event>) {
        self.depth_first_events.push_front(event);
    }

    fn arm_breadth_first(&mut self, event: Box<Event>) {
        self.events.push_back(event);
    }

    /// Run the even loop for `max_turn_count` turns or until there is nothing left to be done,
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
