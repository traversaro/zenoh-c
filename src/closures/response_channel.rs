use crate::{
    transmute::{TransmuteFromHandle, TransmuteIntoHandle},
    z_closure_reply_drop, z_loaned_reply_t, z_owned_closure_reply_t, z_owned_reply_t,
    z_reply_clone, z_reply_null,
};
use libc::c_void;
use std::{
    mem::MaybeUninit,
    sync::mpsc::{Receiver, TryRecvError},
};
/// A closure is a structure that contains all the elements for stateful, memory-leak-free callbacks:
///
/// Closures are not guaranteed not to be called concurrently.
///
/// We guarantee that:
/// - `call` will never be called once `drop` has started.
/// - `drop` will only be called ONCE, and AFTER EVERY `call` has ended.
/// - The two previous guarantees imply that `call` and `drop` are never called concurrently.
#[repr(C)]
pub struct z_owned_reply_channel_closure_t {
    /// An optional pointer to a closure state.
    context: *mut c_void,
    /// A closure body.
    call: Option<
        extern "C" fn(reply: *mut MaybeUninit<z_owned_reply_t>, context: *mut c_void) -> bool,
    >,
    /// An optional drop function that will be called when the closure is dropped.
    drop: Option<extern "C" fn(context: *mut c_void)>,
}

/// Loaned closure.
#[repr(C)]
pub struct z_loaned_reply_channel_closure_t {
    _0: [usize; 3],
}
decl_transmute_handle!(
    z_owned_reply_channel_closure_t,
    z_loaned_reply_channel_closure_t
);

/// A pair of send / receive ends of channel.
#[repr(C)]
pub struct z_owned_reply_channel_t {
    /// Send end of the channel.
    pub send: z_owned_closure_reply_t,
    /// Receive end of the channel.
    pub recv: z_owned_reply_channel_closure_t,
}

/// Drops the channel and resets it to a gravestone state.
#[no_mangle]
pub extern "C" fn z_reply_channel_drop(channel: &mut z_owned_reply_channel_t) {
    z_closure_reply_drop(&mut channel.send);
    z_reply_channel_closure_drop(&mut channel.recv);
}

/// Returns ``true`` if channel is valid, ``false`` if it is in gravestone state.
#[no_mangle]
pub extern "C" fn z_reply_channel_check(this: &z_owned_reply_channel_t) -> bool {
    !this.send.is_empty() && !this.recv.is_empty()
}

/// Constructs a channel in gravestone state.
#[no_mangle]
#[allow(clippy::missing_safety_doc)]
pub unsafe extern "C" fn z_reply_channel_null(this: *mut MaybeUninit<z_owned_reply_channel_t>) {
    let c = z_owned_reply_channel_t {
        send: z_owned_closure_reply_t::empty(),
        recv: z_owned_reply_channel_closure_t::empty(),
    };
    (*this).write(c);
}

unsafe fn get_send_recv_ends(bound: usize) -> (z_owned_closure_reply_t, Receiver<z_owned_reply_t>) {
    if bound == 0 {
        let (tx, rx) = std::sync::mpsc::channel();
        (
            From::from(move |reply: &z_loaned_reply_t| {
                let mut this = MaybeUninit::<z_owned_reply_t>::uninit();
                z_reply_clone(reply, &mut this as *mut MaybeUninit<z_owned_reply_t>);
                let this = this.assume_init();
                if let Err(e) = tx.send(this) {
                    log::error!("Attempted to push onto a closed reply_fifo: {}", e);
                }
            }),
            rx,
        )
    } else {
        let (tx, rx) = std::sync::mpsc::sync_channel(bound);
        (
            From::from(move |reply: &z_loaned_reply_t| {
                let mut this = MaybeUninit::<z_owned_reply_t>::uninit();
                z_reply_clone(reply, &mut this as *mut MaybeUninit<z_owned_reply_t>);
                let this = this.assume_init();
                if let Err(e) = tx.send(this) {
                    log::error!("Attempted to push onto a closed reply_fifo: {}", e);
                }
            }),
            rx,
        )
    }
}
/// Creates a new blocking fifo channel, returned as a pair of closures.
///
/// If `bound` is different from 0, that channel will be bound and apply back-pressure when full.
///
/// The `send` end should be passed as callback to a `z_get()` call.
///
/// The `recv` end is a synchronous closure that will block until either a `z_owned_reply_t` is available,
/// which it will then return; or until the `send` closure is dropped and all replies have been consumed,
/// at which point it will return an invalidated `z_owned_reply_t`, and so will further calls.
#[no_mangle]
#[allow(clippy::missing_safety_doc)]
pub unsafe extern "C" fn zc_reply_fifo_new(
    this: *mut MaybeUninit<z_owned_reply_channel_t>,
    bound: usize,
) {
    let (send, rx) = get_send_recv_ends(bound);
    let c = z_owned_reply_channel_t {
        send,
        recv: From::from(move |this: *mut MaybeUninit<z_owned_reply_t>| {
            if let Ok(val) = rx.recv() {
                (*this).write(val);
            } else {
                z_reply_null(this);
            }
            true
        }),
    };
    (*this).write(c);
}

/// Creates a new non-blocking fifo channel, returned as a pair of closures.
///
/// If `bound` is different from 0, that channel will be bound and apply back-pressure when full.
///
/// The `send` end should be passed as callback to a `z_get()` call.
///
/// The `recv` end is a synchronous closure that will block until either a `z_owned_reply_t` is available,
/// which it will then return; or until the `send` closure is dropped and all replies have been consumed,
/// at which point it will return an invalidated `z_owned_reply_t`, and so will further calls.
#[no_mangle]
#[allow(clippy::missing_safety_doc)]
pub unsafe extern "C" fn zc_reply_non_blocking_fifo_new(
    this: *mut MaybeUninit<z_owned_reply_channel_t>,
    bound: usize,
) {
    let (send, rx) = get_send_recv_ends(bound);
    let c = z_owned_reply_channel_t {
        send,
        recv: From::from(
            move |this: *mut MaybeUninit<z_owned_reply_t>| match rx.try_recv() {
                Ok(val) => {
                    (*this).write(val);
                    true
                }
                Err(TryRecvError::Disconnected) => {
                    z_reply_null(this);
                    true
                }
                Err(TryRecvError::Empty) => {
                    z_reply_null(this);
                    false
                }
            },
        ),
    };
    (*this).write(c);
}

impl z_owned_reply_channel_closure_t {
    pub fn empty() -> Self {
        z_owned_reply_channel_closure_t {
            context: std::ptr::null_mut(),
            call: None,
            drop: None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.call.is_none() && self.drop.is_none() && self.context.is_null()
    }
}
unsafe impl Send for z_owned_reply_channel_closure_t {}
unsafe impl Sync for z_owned_reply_channel_closure_t {}
impl Drop for z_owned_reply_channel_closure_t {
    fn drop(&mut self) {
        if let Some(drop) = self.drop {
            drop(self.context)
        }
    }
}

/// Constructs a gravestone value `z_owned_reply_channel_closure_t` type.
#[no_mangle]
#[allow(clippy::missing_safety_doc)]
pub unsafe extern "C" fn z_reply_channel_closure_null(
    this: *mut MaybeUninit<z_owned_reply_channel_closure_t>,
) {
    (*this).write(z_owned_reply_channel_closure_t::empty());
}

/// Calls the closure. Calling an uninitialized closure is a no-op.
#[no_mangle]
pub extern "C" fn z_reply_channel_closure_call(
    closure: &z_loaned_reply_channel_closure_t,
    reply: *mut MaybeUninit<z_owned_reply_t>,
) -> bool {
    match closure.transmute_ref().call {
        Some(call) => call(reply, closure.transmute_ref().context),
        None => {
            log::error!("Attempted to call an uninitialized closure!");
            true
        }
    }
}

/// Returns ``true`` if closure is valid, ``false`` if it is in gravestone state.
#[no_mangle]
pub extern "C" fn z_reply_channel_closure_check(this: &z_owned_reply_channel_closure_t) -> bool {
    !this.is_empty()
}

/// Drops the closure. Droping an uninitialized closure is a no-op.
#[no_mangle]
pub extern "C" fn z_reply_channel_closure_drop(closure: &mut z_owned_reply_channel_closure_t) {
    let mut empty_closure = z_owned_reply_channel_closure_t::empty();
    std::mem::swap(&mut empty_closure, closure);
}
impl<F: Fn(*mut MaybeUninit<z_owned_reply_t>) -> bool> From<F> for z_owned_reply_channel_closure_t {
    fn from(f: F) -> Self {
        let this = Box::into_raw(Box::new(f)) as _;
        extern "C" fn call<F: Fn(*mut MaybeUninit<z_owned_reply_t>) -> bool>(
            response: *mut MaybeUninit<z_owned_reply_t>,
            this: *mut c_void,
        ) -> bool {
            let this = unsafe { &*(this as *const F) };
            this(response)
        }
        extern "C" fn drop<F>(this: *mut c_void) {
            std::mem::drop(unsafe { Box::from_raw(this as *mut F) })
        }
        z_owned_reply_channel_closure_t {
            context: this,
            call: Some(call::<F>),
            drop: Some(drop::<F>),
        }
    }
}

/// Borrows closure.
#[no_mangle]
pub extern "C" fn z_reply_channel_closure_loan(
    closure: &z_owned_reply_channel_closure_t,
) -> &z_loaned_reply_channel_closure_t {
    closure.transmute_handle()
}
