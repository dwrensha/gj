

#[cfg(target_os = "macos")]
pub mod kqueue;

#[cfg(target_os = "macos")]
pub type Reactor = kqueue::Reactor;
