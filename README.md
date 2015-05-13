# GJ - Good Job promise-based concurrency

*** This is a work in progress! It doesn't do much yet. ***

GJ is a port of the [KJ concurrency framework](https://capnproto.org/cxxrpc.html#kj-concurrency-framework)
into Rust.

A GJ event loop allows you to register callbacks to be executed when nonblocking IO completes.
You can have many theads, each with their own GJ event loop.
A single event loop runs on a single thread,
so you can share mutable data between callbacks without any need for mutexes or atomics.

