# Good Job, promise-based concurrency

[![Build Status](https://travis-ci.org/dwrensha/gj.svg?branch=master)](https://travis-ci.org/dwrensha/gj)
[![crates.io](http://meritbadge.herokuapp.com/gj)](https://crates.io/crates/gj)

[documentation](http://docs.capnproto-rust.org/gj/index.html).

EXPERIMENTAL! UNSTABLE! A WORK IN PROGRESS!

GJ is a port of the
[KJ concurrency framework](https://capnproto.org/cxxrpc.html#kj-concurrency-framework)
into Rust.
A GJ event loop allows you to execute many lightweight stackless tasks on a single operating system thread.
You can share mutable data between tasks without any need for mutexes or atomics.
GJ also provides a completion-based I/O interface.



