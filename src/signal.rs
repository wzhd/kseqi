use std::{io, cell::UnsafeCell, ptr::null_mut, mem, sync::atomic::AtomicI32};

use libc::sigaction;
use mio::unix::pipe;

static PIPE_SENDER: PipeSender = PipeSender(UnsafeCell::new(None));

struct PipeSender(UnsafeCell<Option<pipe::Sender>>);

unsafe impl Sync for PipeSender{}

pub(crate) static LAST_SIG: AtomicI32 = AtomicI32::new(0);

pub(crate) fn quit_recver()-> io::Result<pipe::Receiver>{
    let (s, r): (pipe::Sender, pipe::Receiver)= pipe::new()?;
    let p = PIPE_SENDER.0.get();
    unsafe {
        *p = Some(s);

        let mut sigset = mem::MaybeUninit::uninit();
        libc::sigemptyset(sigset.as_mut_ptr());
        let sigset = sigset.assume_init();
        let os_handler = os_handler as *const extern fn(libc::c_int) as _;
        let sa = sigaction { sa_sigaction: os_handler, sa_mask: sigset, sa_flags: libc::SA_RESTART, sa_restorer: None };
        for s in [
            libc::SIGINT,
            libc::SIGTERM, libc::SIGHUP, ]{
            let _r = libc::sigaction(s, &sa, null_mut());
        }
    };

    Ok((r))

}


extern "C" fn os_handler(i: libc::c_int) {
    LAST_SIG.store(i, std::sync::atomic::Ordering::Relaxed);
    unsafe {
        let p = PIPE_SENDER.0.get();
        let p = &mut *p;
        if let Some(s)= p.take(){
            drop(s);
        }
    }
}
