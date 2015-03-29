

pub type Error = Box<::std::error::Error>;
pub type Result<T> = ::std::result::Result<T, Error>;

pub struct Promise<T> {
    phantom_data : ::std::marker::PhantomData<T>,
    pub node : Box<PromiseNode>,
}

pub trait Event {

    // required methods

    fn fire(&mut self);

    // provided methods

    fn arm_breadth_first(&mut self) {}
    fn arm_depth_first(&mut self) {}
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
        event.arm_breadth_first();
    }
    fn get(&self) {

    }
}

