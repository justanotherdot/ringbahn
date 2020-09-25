use std::os::unix::io::RawFd;
use std::mem::ManuallyDrop;

use iou::sqe::SockAddr;

use super::{Event, SQE, SQEs, Cancellation};

pub struct Connect {
    pub fd: RawFd,
    pub addr: Box<SockAddr>,
}

impl Event for Connect {
    fn sqes_needed(&self) -> u32 { 1 }

    unsafe fn prepare<'sq>(&mut self, sqs: &mut SQEs<'sq>) -> SQE<'sq> {
        let mut sqe = sqs.single().unwrap();
        sqe.prep_connect(self.fd, &mut *self.addr);
        sqe
    }

    unsafe fn cancel(this: &mut ManuallyDrop<Self>) -> Cancellation {
        Cancellation::object(ManuallyDrop::take(this).addr)
    }
}
