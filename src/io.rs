
use Promise;
use std::rc::Rc;

pub trait AsyncRead {
    fn read(buf: Rc<Vec<u8>>, start: usize, min_bytes: usize) -> Promise<u32>;
}


pub trait AsyncWrite {

    // Hm. Seems like the caller is often going to need to do an extra copy here.
    // Can we avoid that somehow?
    fn write(buf: Vec<u8>) -> Promise<()>;
}
