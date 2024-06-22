use std::{
    mem::MaybeUninit,
    sync::{Condvar, Mutex, MutexGuard},
    thread::{self, JoinHandle},
};

use libc::c_void;

pub use crate::opaque_types::z_loaned_mutex_t;
pub use crate::opaque_types::z_owned_mutex_t;
use crate::{
    errors,
    transmute::{
        unwrap_ref_unchecked, unwrap_ref_unchecked_mut, Inplace, TransmuteFromHandle,
        TransmuteIntoHandle, TransmuteRef, TransmuteUninitPtr,
    },
};

decl_transmute_owned!(
    Option<(Mutex<()>, Option<MutexGuard<'static, ()>>)>,
    z_owned_mutex_t,
    z_moved_mutex_t
);
decl_transmute_handle!(
    (Mutex<()>, Option<MutexGuard<'static, ()>>),
    z_loaned_mutex_t
);

/// Constructs a mutex.
/// @return 0 in case of success, negative error code otherwise.
#[no_mangle]
pub extern "C" fn z_mutex_init(this: *mut MaybeUninit<z_owned_mutex_t>) -> errors::z_error_t {
    let this = this.transmute_uninit_ptr();
    let m = (Mutex::<()>::new(()), None::<MutexGuard<'static, ()>>);
    Inplace::init(this, Some(m));
    errors::Z_OK
}

/// Drops mutex and resets it to its gravestone state.
#[no_mangle]
pub extern "C" fn z_mutex_drop(this: z_moved_mutex_t) {
    let _ = this.transmute_mut().extract().take();
}

/// Returns ``true`` if mutex is valid, ``false`` otherwise.
#[no_mangle]
pub extern "C" fn z_mutex_check(this: &z_owned_mutex_t) -> bool {
    this.transmute_ref().is_some()
}

/// Constructs mutex in a gravestone state.
#[no_mangle]
pub extern "C" fn z_mutex_null(this: *mut MaybeUninit<z_owned_mutex_t>) {
    Inplace::empty(this.transmute_uninit_ptr());
}

/// Mutably borrows mutex.
#[no_mangle]
pub extern "C" fn z_mutex_loan_mut(this: &mut z_owned_mutex_t) -> &mut z_loaned_mutex_t {
    let this = this.transmute_mut();
    let this = unwrap_ref_unchecked_mut(this);
    this.transmute_handle_mut()
}

/// Locks mutex. If mutex is already locked, blocks the thread until it aquires the lock.
/// @return 0 in case of success, negative error code in case of failure.
#[no_mangle]
pub extern "C" fn z_mutex_lock(this: &mut z_loaned_mutex_t) -> errors::z_error_t {
    let this = this.transmute_mut();

    match this.0.lock() {
        Ok(new_lock) => {
            let old_lock = this.1.replace(new_lock);
            std::mem::forget(old_lock);
        }
        Err(_) => {
            return errors::Z_EPOISON_MUTEX;
        }
    }
    errors::Z_OK
}

/// Unlocks previously locked mutex. If mutex was not locked by the current thread, the behaviour is undefined.
/// @return 0 in case of success, negative error code otherwise.
#[no_mangle]
pub extern "C" fn z_mutex_unlock(this: &mut z_loaned_mutex_t) -> errors::z_error_t {
    let this = this.transmute_mut();
    if this.1.is_none() {
        return errors::Z_EINVAL_MUTEX;
    } else {
        this.1.take();
    }
    errors::Z_OK
}

/// Tries to lock mutex. If mutex is already locked, return immediately.
/// @return 0 in case of success, negative value if failed to aquire the lock.
#[no_mangle]
#[allow(clippy::missing_safety_doc)]
pub unsafe extern "C" fn z_mutex_try_lock(this: &mut z_loaned_mutex_t) -> errors::z_error_t {
    let this = this.transmute_mut();
    match this.0.try_lock() {
        Ok(new_lock) => {
            let old_lock = this.1.replace(new_lock);
            std::mem::forget(old_lock);
        }
        Err(_) => {
            return errors::Z_EBUSY_MUTEX;
        }
    }
    errors::Z_OK
}

pub use crate::opaque_types::z_loaned_condvar_t;
pub use crate::opaque_types::z_owned_condvar_t;

decl_transmute_owned!(Option<Condvar>, z_owned_condvar_t, z_moved_condvar_t);
decl_transmute_handle!(Condvar, z_loaned_condvar_t);

/// Constructs conditional variable.
#[no_mangle]
pub extern "C" fn z_condvar_init(this: *mut MaybeUninit<z_owned_condvar_t>) {
    let this = this.transmute_uninit_ptr();
    Inplace::init(this, Some(Condvar::new()));
}

/// Constructs conditional variable in a gravestone state.
#[no_mangle]
pub extern "C" fn z_condvar_null(this: *mut MaybeUninit<z_owned_condvar_t>) {
    let this = this.transmute_uninit_ptr();
    Inplace::empty(this);
}

/// Drops conditional variable.
#[no_mangle]
pub extern "C" fn z_condvar_drop(this: z_moved_condvar_t) {
    let _ = this.transmute_mut().extract().take();
}

/// Returns ``true`` if conditional variable is valid, ``false`` otherwise.
#[no_mangle]
pub extern "C" fn z_condvar_check(this: &z_owned_condvar_t) -> bool {
    this.transmute_ref().is_some()
}

/// Borrows conditional variable.
#[no_mangle]
pub extern "C" fn z_condvar_loan(this: &z_owned_condvar_t) -> &z_loaned_condvar_t {
    let this = this.transmute_ref();
    let this = unwrap_ref_unchecked(this);
    this.transmute_handle()
}

/// Mutably borrows conditional variable.
#[no_mangle]
pub extern "C" fn z_condvar_loan_mut(this: &mut z_owned_condvar_t) -> &mut z_loaned_condvar_t {
    let this = this.transmute_mut();
    let this = unwrap_ref_unchecked_mut(this);
    this.transmute_handle_mut()
}

/// Wakes up one blocked thread waiting on this condiitonal variable.
/// @return 0 in case of success, negative error code in case of failure.
#[no_mangle]
pub extern "C" fn z_condvar_signal(this: &z_loaned_condvar_t) -> errors::z_error_t {
    let this = this.transmute_ref();
    this.notify_one();
    errors::Z_OK
}

/// Blocks the current thread until the conditional variable receives a notification.
///
/// The function atomically unlocks the guard mutex `m` and blocks the current thread.
/// When the function returns the lock will have been re-aquired again.
/// Note: The function may be subject to spurious wakeups.
#[no_mangle]
#[allow(clippy::missing_safety_doc)]
pub unsafe extern "C" fn z_condvar_wait(
    this: &z_loaned_condvar_t,
    m: &mut z_loaned_mutex_t,
) -> errors::z_error_t {
    let this = this.transmute_ref();
    let m = m.transmute_mut();
    if m.1.is_none() {
        return errors::Z_EINVAL_MUTEX; // lock was not aquired prior to wait call
    }

    let lock = m.1.take().unwrap();
    match this.wait(lock) {
        Ok(new_lock) => m.1 = Some(new_lock),
        Err(_) => return errors::Z_EPOISON_MUTEX,
    }

    errors::Z_OK
}

pub use crate::opaque_types::z_owned_task_t;

decl_transmute_owned!(Option<JoinHandle<()>>, z_owned_task_t, z_moved_task_t);

#[repr(C)]
#[derive(Clone, Copy)]
pub struct z_task_attr_t(usize);

/// Constructs task in a gravestone state.
#[no_mangle]
pub extern "C" fn z_task_null(this: *mut MaybeUninit<z_owned_task_t>) {
    let this = this.transmute_uninit_ptr();
    Inplace::empty(this);
}

/// Detaches the task and releases all allocated resources.
#[no_mangle]
pub extern "C" fn z_task_detach(this: z_moved_task_t) {
    let _ = this.transmute_mut().extract().take();
}

/// Joins the task and releases all allocated resources
#[no_mangle]
pub extern "C" fn z_task_join(this: z_moved_task_t) -> errors::z_error_t {
    let this = this.transmute_mut().extract().take();
    if let Some(task) = this {
        match task.join() {
            Ok(_) => errors::Z_OK,
            Err(_) => errors::Z_EINVAL_MUTEX,
        }
    } else {
        errors::Z_OK
    }
}

/// Returns ``true`` if task is valid, ``false`` otherwise.
#[no_mangle]
pub extern "C" fn z_task_check(this: &z_owned_task_t) -> bool {
    this.transmute_ref().is_some()
}

struct FunArgPair {
    fun: unsafe extern "C" fn(arg: *mut c_void),
    arg: *mut c_void,
}

impl FunArgPair {
    unsafe fn call(self) {
        (self.fun)(self.arg);
    }
}

unsafe impl Send for FunArgPair {}

/// Constructs a new task.
///
/// @param this_: An uninitialized memory location where task will be constructed.
/// @param _attr: Attributes of the task (currently unused).
/// @param fun: Function to be executed by the task.
/// @param arg: Argument that will be passed to the function `fun`.
#[no_mangle]
#[allow(clippy::missing_safety_doc)]
pub unsafe extern "C" fn z_task_init(
    this: *mut MaybeUninit<z_owned_task_t>,
    _attr: *const z_task_attr_t,
    fun: unsafe extern "C" fn(arg: *mut c_void),
    arg: *mut c_void,
) -> errors::z_error_t {
    let this = this.transmute_uninit_ptr();
    let fun_arg_pair = FunArgPair { fun, arg };

    match thread::Builder::new().spawn(move || fun_arg_pair.call()) {
        Ok(join_handle) => {
            Inplace::init(this, Some(join_handle));
        }
        Err(_) => return errors::Z_EAGAIN_MUTEX,
    }
    errors::Z_OK
}
