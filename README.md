# Good Job Event Loop

[![Build Status](https://travis-ci.org/dwrensha/gj.svg?branch=master)](https://travis-ci.org/dwrensha/gj)
[![crates.io](http://meritbadge.herokuapp.com/gj)](https://crates.io/crates/gj)

[documentation](http://docs.capnproto-rust.org/gj/index.html)

EXPERIMENTAL! UNSTABLE! A WORK IN PROGRESS!

GJ is a port of the
[KJ event loop](https://capnproto.org/cxxrpc.html#kj-concurrency-framework)
into Rust.
Its central abstraction is the
[`Promise<T,E>`](http://docs.capnproto-rust.org/gj/struct.Promise.html) struct
which is similar to Javascript's
[`Promise`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Promise)
class.

Promises that are chained recursively can be thought of as lightweight stackless tasks.
A GJ event loop allows you to execute many such tasks on a single operating system thread
and to safely share mutable data between them without any need for mutexes or atomics.

For a completion-based I/O interface built on top of GJ,
see [gjio](https://github.com/dwrensha/gjio).
