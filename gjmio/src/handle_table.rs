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

use std::collections::binary_heap::BinaryHeap;
use std::ops::{Index, IndexMut};

#[derive(PartialEq, Eq, Copy, Clone, Hash)]
pub struct Handle { pub val : usize }

// Reverse ordering.
impl ::std::cmp::Ord for Handle {
    fn cmp(&self, other : &Handle) -> ::std::cmp::Ordering {
        if self.val > other.val { ::std::cmp::Ordering::Less }
        else if self.val < other.val { ::std::cmp::Ordering::Greater }
        else { ::std::cmp::Ordering::Equal }
    }
}

impl ::std::cmp::PartialOrd for Handle {
    fn partial_cmp(&self, other : &Handle) -> Option<::std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

pub struct HandleTable<T> {
    slots: Vec<Option<T>>,
    num_active: usize,
    // prioritize lower values
    free_ids : BinaryHeap<Handle>,
}

impl <T> HandleTable<T> {
    pub fn new() -> HandleTable<T> {
        HandleTable { slots : Vec::new(),
                      num_active: 0,
                      free_ids : BinaryHeap::new() }
    }

    pub fn remove(&mut self, handle: Handle) -> Option<T> {
        let result = ::std::mem::replace(&mut self.slots[handle.val], None);
        if !result.is_none() {
            self.free_ids.push(handle);
        }
        self.num_active -= 1;
        return result;
    }

    pub fn push(&mut self, val: T) -> Handle {
        self.num_active += 1;
        match self.free_ids.pop() {
            Some(Handle { val: id }) => {
                assert!(self.slots[id as usize].is_none());
                self.slots[id as usize] = Some(val);
                Handle { val: id }
            }
            None => {
                self.slots.push(Some(val));
                Handle { val: self.slots.len() - 1 }
            }
        }
    }

    // Returns the number of currently active handles in the table.
    pub fn len(&self) -> usize {
        self.num_active
    }
}

impl<T> Index<Handle> for HandleTable<T> {
    type Output = T;

    fn index<'a>(&'a self, idx: Handle) -> &'a T {
        match &self.slots[idx.val] {
            &Some(ref v) => return v,
            &None => panic!("invalid handle idx: {}", idx.val),
        }
    }
}

impl<T> IndexMut<Handle> for HandleTable<T> {
    fn index_mut<'a>(&'a mut self, idx: Handle) -> &'a mut T {
        match &mut self.slots[idx.val] {
            &mut Some(ref mut v) => return v,
            &mut None => panic!("invalid handle idx: {}", idx.val),
        }
    }
}
