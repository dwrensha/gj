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



pub enum StepResult<T, S> {
   TheresMore(S),
   Done(T)
}

/// Intermediate state for a reading a T.
pub trait AsyncReadState<T> {

   /// Reads as much as possible without blocking. If done, returns the final T value. Otherwise
   /// returns the new intermediate state T.
   fn read_step<R: ::std::io::Read>(self, r: &mut R) -> ::std::io::Result<StepResult<T,Self>>;
}

/// Gives back `r` once the T has been completely read.
fn read_async<R, S, T>(r: R, state: S) -> Promise<(R, T)>
  where R: ::std::io::Read + 'static,
        S: AsyncReadState<T> + 'static,
        T: 'static {
            unimplemented!();
}
