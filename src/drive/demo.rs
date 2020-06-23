//! A demo driver for experimentation purposes

use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Poll, Context};
use std::thread;

use futures_core::ready;
use once_cell::sync::Lazy;
use parking_lot::Mutex;

const SQ_ENTRIES: u32   = 32;
const CQ_ENTRIES: usize = (SQ_ENTRIES * 2) as usize;

use access_queue::*;

use super::{Drive, Completion};

use crate::kernel::*;

static SQ: Lazy<AccessQueue<Mutex<SubmissionQueue>>> = Lazy::new(init_sq);

/// The driver handle
pub struct DemoDriver<'a> {
    sq: Access<'a, Mutex<SubmissionQueue>>,
}

impl Default for DemoDriver<'_> {
    fn default() -> Self {
        driver()
    }
}

impl<'a> Clone for DemoDriver<'a> {
    fn clone(&self) -> DemoDriver<'a> {
        driver()
    }
}

impl Drive for DemoDriver<'_> {
    fn poll_prepare<'cx>(
        mut self: Pin<&mut Self>,
        ctx: &mut Context<'cx>,
        count: u32,
        prepare: impl FnOnce(SubmissionSegment<'_>, &mut Context<'cx>) -> Completion<'cx>,
    ) -> Poll<Completion<'cx>> {
        // Wait for access to prepare. When ready, create a new Access future to wait next time we
        // want to prepare with this driver, and lock the SQ.
        //
        // TODO likely we should be using a nonblocking mutex?
        let access = ready!(Pin::new(&mut self.sq).poll(ctx));
        let (sq, access) = access.hold_and_reenqueue();
        self.sq = access;
        let mut sq = sq.lock();
        loop {
            match sq.prepare(count) {
                Some(sqs)   => return Poll::Ready(prepare(sqs, ctx)),
                None        => { let _ = sq.submit(); }
            }
        }
    }

    fn poll_submit(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        eager: bool,
    ) -> Poll<io::Result<u32>> {
        let result = if eager {
            self.sq.skip_queue().lock().submit()
        } else {
            Ok(0)
        };
        Poll::Ready(result)
    }
}

/// Construct a demo driver handle
pub fn driver() -> DemoDriver<'static> {
    DemoDriver { sq: SQ.access() }
}

fn init_sq() -> AccessQueue<Mutex<SubmissionQueue>> {
    let ring = IoUring::new(SQ_ENTRIES).expect("TODO handle io_uring_init failure");
    let (sq, cq) = ring.queues();
    thread::spawn(move || complete(cq));
    AccessQueue::new(Mutex::new(sq), CQ_ENTRIES)
}

fn complete(mut cq: CompletionQueue) {
    while let Ok(cqe) = cq.wait_for_cqe() {
        let mut ready = cq.ready() as usize + 1;
        SQ.release(ready);

        cqe.complete();
        ready -= 1;

        while let Some(cqe) = cq.peek_for_cqe() {
            if ready == 0 {
                ready = cq.ready() as usize + 1;
                SQ.release(ready);
            }

            super::complete(cqe.into());
            ready -= 1;
        }

        debug_assert!(ready == 0);
    }
}
