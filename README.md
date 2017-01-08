# Good Job Event Loop

[![Build Status](https://travis-ci.org/dwrensha/gj.svg?branch=master)](https://travis-ci.org/dwrensha/gj)
[![crates.io](http://meritbadge.herokuapp.com/gj)](https://crates.io/crates/gj)

[documentation](https://docs.rs/gj/)

WARNING:
as of the [0.8 relase of capnp-rpc-rust](https://dwrensha.github.io/capnproto-rust/2017/01/04/rpc-futures.html),
this library is DEPRECATED. Please use [futures-rs](https://github.com/alexcrichton/futures-rs) instead.

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
