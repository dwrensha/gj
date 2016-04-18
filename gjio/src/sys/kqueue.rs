use std::os::unix::io::RawFd;
use gj::{Promise, PromiseFulfiller};
use handle_table::{HandleTable, Handle};

pub struct FdObserver {
    read_fulfiller: Option<PromiseFulfiller<(), ::std::io::Error>>,
    write_fulfiller: Option<PromiseFulfiller<(), ::std::io::Error>>,
}

impl FdObserver {
    pub fn when_becomes_readable(&mut self) -> Promise<(), ::std::io::Error> {
        let (promise, fulfiller) = Promise::and_fulfiller();
        self.read_fulfiller = Some(fulfiller);
        promise
    }

    pub fn when_becomes_writable(&mut self) -> Promise<(), ::std::io::Error> {
        let (promise, fulfiller) = Promise::and_fulfiller();
        self.write_fulfiller = Some(fulfiller);
        promise
    }
}

pub struct Reactor {
    pub kqueue: RawFd,
    pub observers: HandleTable<FdObserver>,
}

impl Reactor {
    pub fn new() -> Result<Reactor, ::std::io::Error> {
        Ok(Reactor {
            kqueue: try!(::nix::sys::event::kqueue()),
            observers: HandleTable::new(),
        })
    }

    pub fn run_once(&mut self) -> Result<(), ::std::io::Error> {
        unimplemented!();
        // call kevent() ...
    }


    pub fn new_observer(&mut self, fd: RawFd) -> Result<Handle, ::std::io::Error> {
        let observer = FdObserver { read_fulfiller: None, write_fulfiller: None };
        let result = self.observers.push(observer);
        Ok(result)
    }
}
/*
impl ::mio::Handler for Handler {
    type Timeout = Timeout;
    type Message = ();
    fn ready(&mut self, _event_loop: &mut ::mio::EventLoop<Handler>,
             token: ::mio::Token, events: ::mio::EventSet) {
        if events.is_readable() {
            match ::std::mem::replace(&mut self.observers[Handle {val: token.0}].read_fulfiller, None) {
                Some(fulfiller) => {
                    fulfiller.fulfill(())
                }
                None => {
                    ()
                }
            }
        }
        if events.is_writable() {
            match ::std::mem::replace(&mut self.observers[Handle { val: token.0}].write_fulfiller, None) {
                Some(fulfiller) => fulfiller.fulfill(()),
                None => (),
            }
        }
    }
    fn timeout(&mut self, _event_loop: &mut ::mio::EventLoop<Handler>, timeout: Timeout) {
        timeout.fulfiller.fulfill(());
    }
}

struct Timeout {
    fulfiller: PromiseFulfiller<(), ::std::io::Error>,
}
*/
