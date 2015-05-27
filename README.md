# Good Job, promise-based concurrency

[![Build Status](https://travis-ci.org/dwrensha/gj.svg?branch=master)](https://travis-ci.org/dwrensha/gj)

[Documentation here](http://docs.capnproto-rust.org/gj/index.html).

EXPERIMENTAL! UNSTABLE! A WORK IN PROGRESS!

GJ is a port of the
[KJ concurrency framework](https://capnproto.org/cxxrpc.html#kj-concurrency-framework)
into Rust.

A GJ event loop allows you to register callbacks to be executed when nonblocking IO completes.
You can have many threads, each with their own GJ event loop.
A single event loop runs on a single thread,
so you can share mutable data between callbacks without any need for mutexes or atomics.

