// SPDX-License-Identifier: Apache-2.0
//! Reference counter for channels.

// This file is taken from crossbeam-channels, with modifications to be nostd.

extern crate alloc;

use crate::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use alloc::boxed::Box;
use core::ops;
use core::ptr::NonNull;

/// Reference counter internals.
struct Counter<C> {
    /// The number of senders associated with the channel.
    senders: AtomicUsize,

    /// The number of receivers associated with the channel.
    receivers: AtomicUsize,

    /// Set to `true` if the last sender or the last receiver reference deallocates the channel.
    destroy: AtomicBool,

    /// The internal channel.
    chan: C,
}

/// Wraps a channel into the reference counter.
pub(crate) fn new<C>(chan: C) -> (Sender<C>, Receiver<C>) {
    let counter = NonNull::from(Box::leak(Box::new(Counter {
        senders: AtomicUsize::new(1),
        receivers: AtomicUsize::new(1),
        destroy: AtomicBool::new(false),
        chan,
    })));
    let s = Sender { counter };
    let r = Receiver { counter };
    (s, r)
}

/// The sending side.
pub(crate) struct Sender<C> {
    counter: NonNull<Counter<C>>,
}

impl<C> Sender<C> {
    /// Returns the internal `Counter`.
    fn counter(&self) -> &Counter<C> {
        unsafe { self.counter.as_ref() }
    }

    /// Acquires another sender reference.
    pub(crate) fn acquire(&self) -> Self {
        let count = self.counter().senders.fetch_add(1, Ordering::Relaxed);

        // Cloning senders and calling `mem::forget` on the clones could potentially overflow the
        // counter. It's very difficult to recover sensibly from such degenerate scenarios so we
        // just abort when the count becomes very large.
        if count > isize::MAX as usize {
            // TODO: We need some kind of equivalent here.
            unimplemented!();
        }

        Self {
            counter: self.counter,
        }
    }

    /// Releases the sender reference.
    ///
    /// Function `disconnect` will be called if this is the last sender reference.
    pub(crate) unsafe fn release<F: FnOnce(&C) -> bool>(&self, disconnect: F) {
        if self.counter().senders.fetch_sub(1, Ordering::AcqRel) == 1 {
            disconnect(&self.counter().chan);

            if self.counter().destroy.swap(true, Ordering::AcqRel) {
                drop(unsafe { Box::from_raw(self.counter.as_ptr()) });
            }
        }
    }
}

impl<C> ops::Deref for Sender<C> {
    type Target = C;

    fn deref(&self) -> &C {
        &self.counter().chan
    }
}

impl<C> PartialEq for Sender<C> {
    fn eq(&self, other: &Self) -> bool {
        self.counter == other.counter
    }
}

/// The receiving side.
pub(crate) struct Receiver<C> {
    counter: NonNull<Counter<C>>,
}

impl<C> Receiver<C> {
    /// Returns the internal `Counter`.
    fn counter(&self) -> &Counter<C> {
        unsafe { self.counter.as_ref() }
    }

    /// Acquires another receiver reference.
    pub(crate) fn acquire(&self) -> Self {
        let count = self.counter().receivers.fetch_add(1, Ordering::Relaxed);

        // Cloning receivers and calling `mem::forget` on the clones could potentially overflow the
        // counter. It's very difficult to recover sensibly from such degenerate scenarios so we
        // just abort when the count becomes very large.
        if count > isize::MAX as usize {
            unimplemented!();
        }

        Self {
            counter: self.counter,
        }
    }

    /// Releases the receiver reference.
    ///
    /// Function `disconnect` will be called if this is the last receiver reference.
    pub(crate) unsafe fn release<F: FnOnce(&C) -> bool>(&self, disconnect: F) {
        if self.counter().receivers.fetch_sub(1, Ordering::AcqRel) == 1 {
            disconnect(&self.counter().chan);

            if self.counter().destroy.swap(true, Ordering::AcqRel) {
                drop(unsafe { Box::from_raw(self.counter.as_ptr()) });
            }
        }
    }
}

impl<C> ops::Deref for Receiver<C> {
    type Target = C;

    fn deref(&self) -> &C {
        &self.counter().chan
    }
}

impl<C> PartialEq for Receiver<C> {
    fn eq(&self, other: &Self) -> bool {
        self.counter == other.counter
    }
}
