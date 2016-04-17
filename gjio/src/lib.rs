#[macro_use]
extern crate gj;
extern crate mio;
extern crate nix;

use gj::{Promise, PromiseFulfiller};

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

