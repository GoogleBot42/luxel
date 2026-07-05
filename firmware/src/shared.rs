//! State shared between the web server and the render task.

use alloc::string::String;
use core::cell::RefCell;
use core::sync::atomic::AtomicU32;

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex as BlockingMutex;
use embassy_sync::channel::Channel;

/// Newly uploaded pattern source, already compile-checked by the HTTP
/// handler. The render task recompiles it (compilation is cheap and this
/// keeps `Engine` off the channel — only the `String` has to be `Send`).
pub static CODE_QUEUE: Channel<CriticalSectionRawMutex, String, 2> = Channel::new();

/// Frames rendered in the last full second, updated by the render task.
pub static FPS: AtomicU32 = AtomicU32::new(0);

/// Most recent runtime (vmerr) message with source location, cleared on
/// successful frames after a new upload.
pub static LAST_VMERR: BlockingMutex<CriticalSectionRawMutex, RefCell<Option<String>>> =
    BlockingMutex::new(RefCell::new(None));

pub fn set_vmerr(msg: Option<String>) {
    LAST_VMERR.lock(|c| *c.borrow_mut() = msg);
}

pub fn get_vmerr() -> Option<String> {
    LAST_VMERR.lock(|c| c.borrow().clone())
}
