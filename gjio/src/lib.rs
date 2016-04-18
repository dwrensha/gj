#[macro_use]
extern crate gj;
extern crate nix;

use std::cell::{RefCell};
use std::rc::Rc;
use gj::{Promise, PromiseFulfiller};
use handle_table::{HandleTable, Handle};

mod handle_table;
mod sys;

/// A nonblocking input bytestream.
pub trait Read {
    /// Attempts to read `buf.len()` bytes from the stream, writing them into `buf`.
    /// Returns `self`, the modified `buf`, and the number of bytes actually read.
    /// Returns as soon as `min_bytes` are read or EOF is encountered.
    fn try_read<T>(&mut self, buf: T, min_bytes: usize) -> Promise<(T, usize), ::std::io::Error>
        where T: AsMut<[u8]>;

    /// Like `try_read()`, but returns an error if EOF is encountered before `min_bytes`
    /// can be read.
    fn read<T>(&mut self, buf: T, min_bytes: usize) -> Promise<(T, usize), ::std::io::Error>
        where T: AsMut<[u8]>
    {
        self.try_read(buf, min_bytes).map(move |(buf, n)| {
            if n < min_bytes {
                Err(::std::io::Error::new(::std::io::ErrorKind::Other, "Premature EOF"))
            } else {
                Ok((buf, n))
            }
        })
    }
}

/// A nonblocking output bytestream.
pub trait AsyncWrite {
    /// Attempts to write all `buf.len()` bytes from `buf` into the stream. Returns `self` and `buf`
    /// once all of the bytes have been written.
    fn write<T: AsRef<[u8]>>(&mut self, buf: T) -> Promise<T, ::std::io::Error>;
}

#[cfg(unix)]
type RawDescriptor = std::os::unix::io::RawFd;

pub struct EventPort {
    reactor: Rc<RefCell<::sys::Reactor>>,
}

impl EventPort {
    pub fn new() -> Result<EventPort, ::std::io::Error> {
        Ok( EventPort {
            reactor: Rc::new(RefCell::new(try!(sys::Reactor::new()))),
        })
    }

    pub fn get_network(&self) -> Network {
        Network::new(self.reactor.clone())
    }
}


impl gj::EventPort<::std::io::Error> for EventPort {
    fn wait(&mut self) -> Result<(), ::std::io::Error> {
        self.reactor.borrow_mut().run_once()
    }
}

pub struct Network {
    reactor: Rc<RefCell<::sys::Reactor>>,
}

impl Network {
    fn new(reactor: Rc<RefCell<::sys::Reactor>>) -> Network {
        Network { reactor: reactor }
    }


    fn connect_internal(&mut self, addr: ::nix::sys::socket::SockAddr)
                        -> Promise<SocketStream, ::std::io::Error>
    {
        let reactor = self.reactor.clone();
        Promise::ok(()).then(move |()| {
            let mut reactor = reactor;

            let fd = pry!(nix::sys::socket::socket(addr.family(), nix::sys::socket::SockType::Stream,
                                                   nix::sys::socket::SOCK_NONBLOCK, 0));

            pry!(::nix::sys::socket::connect(fd, &addr));
            let handle = pry!(reactor.borrow_mut().new_observer(fd));

            // TODO: if we're not already connected, maybe only register writable interest,
            // and then reregister with read/write interested once we successfully connect.

            let promise = reactor.borrow_mut().observers[handle].when_becomes_writable();
            promise.map(move |()| {
//                try!(stream.take_socket_error());
                Ok(SocketStream::new(reactor, handle, fd))
            })
        })
    }

    pub fn connect(&mut self, addr: ::std::net::SocketAddr) -> Promise<SocketStream, ::std::io::Error> {
        self.connect_internal(
            ::nix::sys::socket::SockAddr::Inet(::nix::sys::socket::InetAddr::from_std(&addr)))
    }

    fn bind_internal(&mut self, addr: ::nix::sys::socket::SockAddr)
                     -> Result<SocketListener, ::std::io::Error> {
        let fd = try!(nix::sys::socket::socket(addr.family(), nix::sys::socket::SockType::Stream,
                                               nix::sys::socket::SOCK_NONBLOCK, 0));

        try!(nix::sys::socket::bind(fd, &addr));
        let handle = try!(self.reactor.borrow_mut().new_observer(fd));
        Ok(SocketListener::new(self.reactor.clone(), handle, fd))
    }

    fn bind(&mut self, addr: ::std::net::SocketAddr)
            -> Result<SocketListener, ::std::io::Error>
    {
        self.bind_internal(::nix::sys::socket::SockAddr::Inet(::nix::sys::socket::InetAddr::from_std(&addr)))
    }
}

struct SocketListenerInner {
    reactor: Rc<RefCell<::sys::Reactor>>,
    handle: Handle,
    descriptor: RawDescriptor,
}

impl Drop for SocketListenerInner {
    fn drop(&mut self) {
        self.reactor.borrow_mut().observers.remove(self.handle);
    }
}

pub struct SocketListener {
    inner: Rc<RefCell<SocketListenerInner>>,
}

impl Clone for SocketListener {
    fn clone(&self) -> SocketListener {
        SocketListener { inner: self.inner.clone() }
    }
}

impl SocketListener {
    fn new(reactor: Rc<RefCell<::sys::Reactor>>, handle: Handle, descriptor: RawDescriptor)
           -> SocketListener
    {
        SocketListener {
            inner: Rc::new(RefCell::new(SocketListenerInner {
                reactor: reactor,
                handle: handle,
                descriptor: descriptor,
            })),
        }
    }

    fn accept_internal(reactor: Rc<RefCell<SocketListenerInner>>) -> Promise<SocketStream, ::std::io::Error> {
        unimplemented!()
/*        match ::nix::sys::socket::accept4(self.fd, nix::sys::socket::SOCK_NONBLOCK) {
            Ok(fd) => {
                let handle = pry!(reactor.new_observer(fd));
                Promise::ok(SocketStream::new(reactor, handle, fd))
            }
            Err(e) => {
                match e.errno() {
                    ::nix::Errno::EAGAIN | nix::Errno::EWOULDBLOCK => {
                        unimplemented!()
                    }
                    _ => {
                        Promise::err(e.into())
                    }
                }
            }
        }*/
    }

    pub fn accept(&mut self) -> Promise<SocketStream, ::std::io::Error> {
        SocketListener::accept_internal(self.inner.clone())
    }
}

struct SocketStreamInner {
    reactor: Rc<RefCell<::sys::Reactor>>,
    handle: Handle,
    descriptor: RawDescriptor,
}

impl Drop for SocketStreamInner {
    fn drop(&mut self) {
        self.reactor.borrow_mut().observers.remove(self.handle);
    }
}

pub struct SocketStream {
    inner: Rc<RefCell<SocketStreamInner>>,
}

impl Clone for SocketStream {
    fn clone(&self) -> SocketStream {
        SocketStream { inner: self.inner.clone() }
    }
}

impl SocketStream {
    fn new(reactor: Rc<RefCell<::sys::Reactor>>, handle: Handle, descriptor: RawDescriptor) -> SocketStream {
        SocketStream {
            inner: Rc::new(RefCell::new(SocketStreamInner {
                reactor: reactor,
                handle: handle,
                descriptor: descriptor,
            })),
        }
    }
}


