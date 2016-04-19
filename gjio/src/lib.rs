#[macro_use]
extern crate gj;
extern crate nix;

use std::cell::{RefCell};
use std::rc::Rc;
use gj::{Promise};
use handle_table::{Handle};

mod handle_table;
mod sys;

/// A nonblocking input bytestream.
pub trait AsyncRead {
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

pub struct Slice<T> where T: AsRef<[u8]> {
    pub buf: T,
    pub end: usize,
}

impl <T> Slice<T> where T: AsRef<[u8]> {
    pub fn new(buf: T, end: usize) -> Slice<T> {
        Slice { buf: buf, end: end }
    }
}

impl <T> AsRef<[u8]> for Slice<T> where T: AsRef<[u8]> {
    fn as_ref<'a>(&'a self) -> &'a [u8] {
        &self.buf.as_ref()[0..self.end]
    }
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


    pub fn get_tcp_address(&self, addr: ::std::net::SocketAddr) -> SocketAddress {
        SocketAddress::new(self.reactor.clone(),
                            ::nix::sys::socket::SockAddr::Inet(::nix::sys::socket::InetAddr::from_std(&addr)))
    }

    pub fn get_unix_address<P: AsRef<::std::path::Path>>(&self, addr: P)
                            -> Result<SocketAddress, ::std::io::Error>
    {
        // In what situations does this fail?
        Ok(SocketAddress::new(self.reactor.clone(),
                               ::nix::sys::socket::SockAddr::Unix(
                                   try!(::nix::sys::socket::UnixAddr::new(addr.as_ref())))))
    }
}

struct SocketListenerInner {
    reactor: Rc<RefCell<::sys::Reactor>>,
    handle: Handle,
    descriptor: RawDescriptor,
    queue: Option<Promise<(),()>>,
}


pub struct SocketAddress {
    reactor: Rc<RefCell<::sys::Reactor>>,
    addr: ::nix::sys::socket::SockAddr,
}

impl SocketAddress {
    fn new(reactor: Rc<RefCell<::sys::Reactor>>, addr: ::nix::sys::socket::SockAddr)
           -> SocketAddress
    {
        SocketAddress {
            reactor: reactor,
            addr: addr,
        }
    }

    pub fn connect(&self) -> Promise<SocketStream, ::std::io::Error>
    {
        let reactor = self.reactor.clone();
        let addr = self.addr;
        Promise::ok(()).then(move |()| {
            let reactor = reactor;
            let fd = pry!(nix::sys::socket::socket(addr.family(), nix::sys::socket::SockType::Stream,
                                                   nix::sys::socket::SOCK_NONBLOCK, 0));
            loop {
                match ::nix::sys::socket::connect(fd, &addr) {
                    Ok(()) => break,
                    Err(e) if e.errno() == ::nix::Errno::EINTR => continue,
                    Err(e) if e.errno() == ::nix::Errno::EINPROGRESS => break,
                    Err(e) => return Promise::err(e.into()),
                }
            }

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

    pub fn listen(&mut self) -> Result<SocketListener, ::std::io::Error>
    {
        let fd = try!(nix::sys::socket::socket(self.addr.family(), nix::sys::socket::SockType::Stream,
                                               nix::sys::socket::SOCK_NONBLOCK | nix::sys::socket::SOCK_CLOEXEC,
                                               0));

        try!(::nix::sys::socket::setsockopt(fd, nix::sys::socket::sockopt::ReuseAddr, &true));
        try!(nix::sys::socket::bind(fd, &self.addr));
        try!(nix::sys::socket::listen(fd, 1024));


        let handle = try!(self.reactor.borrow_mut().new_observer(fd));
        Ok(SocketListener::new(self.reactor.clone(), handle, fd))
    }
}


impl Drop for SocketListenerInner {
    fn drop(&mut self) {
        let _ = ::nix::unistd::close(self.descriptor);
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
                queue: None,
            })),
        }
    }

    fn accept_internal(inner: Rc<RefCell<SocketListenerInner>>) -> Promise<RawDescriptor, ::std::io::Error> {
        let fd = inner.borrow_mut().descriptor;
        match ::nix::sys::socket::accept4(fd, nix::sys::socket::SOCK_NONBLOCK | nix::sys::socket::SOCK_CLOEXEC) {
            Ok(fd) => {
                Promise::ok(fd)
            }
            Err(e) => {
                match e.errno() {
                    ::nix::Errno::EAGAIN => {
                        let handle = inner.borrow().handle;
                        let promise = {
                            let reactor = &inner.borrow().reactor;
                            let promise = // LOL borrow checker fail.
                                reactor.borrow_mut().observers[handle].when_becomes_readable();
                            promise
                        };
                        promise.then(|()| {
                            SocketListener::accept_internal(inner)
                        })
                    }
                    _ => {
                        Promise::err(e.into())
                    }
                }
            }
        }
    }

    pub fn accept(&mut self) -> Promise<SocketStream, ::std::io::Error> {
        let inner = self.inner.clone();
        let inner2 = inner.clone();
        let maybe_queue = inner.borrow_mut().queue.take();
        let promise = match maybe_queue {
            None => SocketListener::accept_internal(inner2),
            Some(queue) => {
                queue.then_else(move |_| SocketListener::accept_internal(inner2) )
            }
        };

        let (p, f) = Promise::and_fulfiller();
        inner.borrow_mut().queue = Some(p);

        promise.map(move |fd| {
            f.resolve(Ok(()));
            let reactor = inner.borrow().reactor.clone();
            let handle = try!(reactor.borrow_mut().new_observer(fd));
            Ok(SocketStream::new(reactor, handle, fd))
        })
    }
}

struct SocketStreamInner {
    reactor: Rc<RefCell<::sys::Reactor>>,
    handle: Handle,
    descriptor: RawDescriptor,

    read_queue: Option<Promise<(),()>>,
    write_queue: Option<Promise<(),()>>,
}

impl Drop for SocketStreamInner {
    fn drop(&mut self) {
        let _ = ::nix::unistd::close(self.descriptor);
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
                read_queue: None,
                write_queue: None,
            })),
        }
    }

    fn try_read_internal<T>(inner: Rc<RefCell<SocketStreamInner>>,
                            mut buf: T,
                            mut already_read: usize,
                            min_bytes: usize)
                            -> Promise<(T, usize), ::std::io::Error>
        where T: AsMut<[u8]>
    {
        while already_read < min_bytes {
            let descriptor = inner.borrow().descriptor;
            match ::nix::unistd::read(descriptor, &mut buf.as_mut()[already_read..]) {
                Ok(0) => {
                    // EOF
                    return Promise::ok((buf, already_read));
                }
                Ok(n) => {
                    already_read += n;
                }
                Err(e) => {
                    match e.errno() {
                        ::nix::Errno::EINTR => continue,
                        ::nix::Errno::EAGAIN => {
                            let handle = inner.borrow().handle;
                            let promise = {
                                let reactor = &inner.borrow().reactor;
                                let promise = // LOL borrow checker fail.
                                    reactor.borrow_mut().observers[handle].when_becomes_readable();
                                promise
                            };

                            return promise.then_else(move |r| match r {
                                Ok(()) => SocketStream::try_read_internal(inner, buf, already_read, min_bytes),
                                Err(e) => Promise::err(e)
                            });
                        }
                        _ => {
                            return Promise::err(e.into())
                        }
                    }
                }
            }
        }
        Promise::ok((buf, already_read))
    }

    fn write_internal<T>(inner: Rc<RefCell<SocketStreamInner>>,
                         buf: T,
                         mut already_written: usize) -> Promise<T, ::std::io::Error>
        where T: AsRef<[u8]>
    {
        while already_written < buf.as_ref().len() {
            let descriptor = inner.borrow().descriptor;
            match ::nix::unistd::write(descriptor, &buf.as_ref()[already_written..]) {
                Ok(n) => {
                    already_written += n;
                }
                Err(e) => {
                    match e.errno() {
                        ::nix::Errno::EINTR => continue,
                        ::nix::Errno::EAGAIN => {
                            let handle = inner.borrow().handle;
                            let promise = {
                                let reactor = &inner.borrow().reactor;
                                let promise = // LOL borrow checker fail.
                                    reactor.borrow_mut().observers[handle].when_becomes_writable();
                                promise
                            };
                            return promise.then_else(move |r| match r {
                                Ok(()) => SocketStream::write_internal(inner, buf, already_written),
                                Err(e) => Promise::err(e),
                            })
                        }
                        _ => {
                            return Promise::err(e.into());
                        }
                    }
                }
            }
        }
        Promise::ok(buf)
    }
}


impl AsyncRead for SocketStream {
    fn try_read<T>(&mut self, buf: T, min_bytes: usize) -> Promise<(T, usize), ::std::io::Error>
        where T: AsMut<[u8]>
    {
        let inner = self.inner.clone();
        let inner2 = inner.clone();
        let maybe_queue = inner.borrow_mut().read_queue.take();
        let promise = match maybe_queue {
            None => SocketStream::try_read_internal(inner2, buf, 0, min_bytes),
            Some(queue) => {
                queue.then_else(move |_| SocketStream::try_read_internal(inner2, buf, 0, min_bytes) )
            }
        };

        let (p, f) = Promise::and_fulfiller();
        inner.borrow_mut().read_queue = Some(p);

        promise.map_else(move |r| {
            f.resolve(Ok(()));
            r
        })
    }
}

impl AsyncWrite for SocketStream {
    fn write<T>(&mut self, buf: T) -> Promise<T, ::std::io::Error>
        where T: AsRef<[u8]>
    {
        let inner = self.inner.clone();
        let inner2 = inner.clone();
        let maybe_queue = inner.borrow_mut().write_queue.take();
        let promise = match maybe_queue {
            None => SocketStream::write_internal(inner2, buf, 0),
            Some(queue) => {
                queue.then_else(move |_| SocketStream::write_internal(inner2, buf, 0) )
            }
        };

        let (p, f) = Promise::and_fulfiller();
        inner.borrow_mut().write_queue = Some(p);

        promise.map_else(move |r| {
            f.resolve(Ok(()));
            r
        })
    }
}
