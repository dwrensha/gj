#![allow(dead_code)]

pub type Error = Box<::std::error::Error>;
pub type Result<T> = ::std::result::Result<T, Error>;

pub struct Promise<T> {
    phantom_data : ::std::marker::PhantomData<T>,
    pub node : Box<PromiseNode>,
}

pub trait Event {
    fn fire(&mut self);
}

pub trait PromiseNode {
    /// Arms the given event when ready.
    fn on_ready(&self, event : &mut Event);

    fn set_self_pointer(&mut self) {}
    fn get(&self);
}

/// A promise that has already been resolved to an immediate value or error.
pub struct ImmediatePromiseNode<T> {
    result : ::std::result::Result<T, Error>,
}

impl <T> PromiseNode for ImmediatePromiseNode<T> {
    fn on_ready(&self, event : &mut Event) {
        unimplemented!();
        //event.arm_breadth_first();
    }
    fn get(&self) {

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
struct EventLoop {
    daemons: TaskSetImpl,
    running: bool,
    last_runnable_state: bool,
    events: ::std::collections::VecDeque<Box<Event>>,
    depth_first_events: ::std::collections::VecDeque<Box<Event>>,
}

impl EventLoop {

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
        while(!self.depth_first_events.is_empty()) {
            self.events.push_front(self.depth_first_events.pop_back().unwrap());
        }
        return true;
    }
}
