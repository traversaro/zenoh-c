//
// Copyright (c) 2023 ZettaScale Technology
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ZettaScale Zenoh Team, <zenoh@zettascale.tech>
//

use std::mem::MaybeUninit;
use zenoh::{
    liveliness::{Liveliness, LivelinessToken},
    prelude::SessionDeclarations,
};

use crate::transmute::TransmuteIntoHandle;
use crate::{
    errors,
    transmute::{
        unwrap_ref_unchecked, Inplace, TransmuteFromHandle, TransmuteRef, TransmuteUninitPtr,
    },
    z_closure_reply_call, z_closure_sample_call, z_loaned_keyexpr_t, z_loaned_session_t,
    z_owned_closure_reply_t, z_owned_closure_sample_t, z_owned_subscriber_t,
};
use crate::{z_closure_reply_loan, z_closure_sample_loan};
use zenoh::core::Wait;

use crate::opaque_types::zc_loaned_liveliness_token_t;
use crate::opaque_types::zc_owned_liveliness_token_t;
decl_transmute_owned!(
    Option<LivelinessToken<'static>>,
    zc_owned_liveliness_token_t,
    z_moved_lveliness_token_t
);
decl_transmute_handle!(LivelinessToken<'static>, zc_loaned_liveliness_token_t);
validate_equivalence!(zc_owned_liveliness_token_t, zc_loaned_liveliness_token_t);

/// Constructs liveliness token in its gravestone state.
#[no_mangle]
pub extern "C" fn zc_liveliness_token_null(this: *mut MaybeUninit<zc_owned_liveliness_token_t>) {
    let this = this.transmute_uninit_ptr();
    Inplace::empty(this);
}

/// Returns ``true`` if liveliness token is valid, ``false`` otherwise.
#[no_mangle]
pub extern "C" fn zc_liveliness_token_check(this: &zc_owned_liveliness_token_t) -> bool {
    this.transmute_ref().is_some()
}

/// Undeclares liveliness token, frees memory and resets it to a gravestone state.
#[no_mangle]
pub extern "C" fn zc_liveliness_token_drop(this: &mut zc_owned_liveliness_token_t) {
    let this = this.transmute_mut();
    Inplace::drop(this);
}
/// The options for `zc_liveliness_declare_token()`.
#[repr(C)]
pub struct zc_liveliness_declaration_options_t {
    _dummy: u8,
}

/// Constructs default value for `zc_liveliness_declaration_options_t`.
#[no_mangle]
pub extern "C" fn zc_liveliness_declaration_options_default(
    this: &mut zc_liveliness_declaration_options_t,
) {
    *this = zc_liveliness_declaration_options_t { _dummy: 0 };
}

/// Borrows token.
#[no_mangle]
extern "C" fn zc_liveliness_token_loan(
    this: &zc_owned_liveliness_token_t,
) -> &zc_loaned_liveliness_token_t {
    unwrap_ref_unchecked(this.transmute_ref()).transmute_handle()
}

/// Constructs and declares a liveliness token on the network.
///
/// Liveliness token subscribers on an intersecting key expression will receive a PUT sample when connectivity
/// is achieved, and a DELETE sample if it's lost.
///
/// @param this_: An uninitialized memory location where liveliness token will be constructed.
/// @param session: A Zenos session to declare the liveliness token.
/// @param key_expr: A keyexpr to declare a liveliess token for.
/// @param _options: Liveliness token declaration properties.
#[no_mangle]
pub extern "C" fn zc_liveliness_declare_token(
    this: *mut MaybeUninit<zc_owned_liveliness_token_t>,
    session: &z_loaned_session_t,
    key_expr: &z_loaned_keyexpr_t,
    _options: Option<&zc_liveliness_declaration_options_t>,
) -> errors::z_error_t {
    let this = this.transmute_uninit_ptr();
    let session = session.transmute_ref();
    let key_expr = key_expr.transmute_ref();
    match session.liveliness().declare_token(key_expr).wait() {
        Ok(token) => {
            Inplace::init(this, Some(token));
            errors::Z_OK
        }
        Err(e) => {
            log::error!("Failed to undeclare token: {e}");
            Inplace::empty(this);
            errors::Z_EGENERIC
        }
    }
}

/// Destroys a liveliness token, notifying subscribers of its destruction.
#[no_mangle]
pub extern "C" fn zc_liveliness_undeclare_token(
    this: &mut zc_owned_liveliness_token_t,
) -> errors::z_error_t {
    let this = this.transmute_mut();
    if let Some(token) = this.extract().take() {
        if let Err(e) = token.undeclare().wait() {
            log::error!("Failed to undeclare token: {e}");
            return errors::Z_EGENERIC;
        }
    }
    errors::Z_OK
}

/// The options for `zc_liveliness_declare_subscriber()`
#[repr(C)]
pub struct zc_liveliness_subscriber_options_t {
    _dummy: u8,
}

/// Constucts default value for `zc_liveliness_declare_subscriber_options_t`.
#[no_mangle]
pub extern "C" fn zc_liveliness_subscriber_options_default(
    this: &mut zc_liveliness_subscriber_options_t,
) {
    *this = zc_liveliness_subscriber_options_t { _dummy: 0 };
}

/// Declares a subscriber on liveliness tokens that intersect `key_expr`.
///
/// @param this_: An unitialized memory location where subscriber will be constructed.
/// @param session: The Zenoh session.
/// @param key_expr: The key expression to subscribe to.
/// @param callback: The callback function that will be called each time a liveliness token status is changed.
/// @param _options: The options to be passed to the liveliness subscriber declaration.
///
/// @return 0 in case of success, negative error values otherwise.
#[no_mangle]
pub extern "C" fn zc_liveliness_declare_subscriber(
    this: *mut MaybeUninit<z_owned_subscriber_t>,
    session: &z_loaned_session_t,
    key_expr: &z_loaned_keyexpr_t,
    callback: z_moved_closure_sample_t,
    _options: Option<&mut zc_liveliness_subscriber_options_t>,
) -> errors::z_error_t {
    let this = this.transmute_uninit_ptr();
    let session = session.transmute_ref();
    let callback = core::mem::replace(callback, z_owned_closure_sample_t::empty());
    let key_expr = key_expr.transmute_ref();
    match session
        .liveliness()
        .declare_subscriber(key_expr)
        .callback(move |sample| {
            let sample = sample.transmute_handle();
            z_closure_sample_call(z_closure_sample_loan(&callback), sample)
        })
        .wait()
    {
        Ok(subscriber) => {
            Inplace::init(this, Some(subscriber));
            errors::Z_OK
        }
        Err(e) => {
            log::error!("Failed to subscribe to liveliness: {e}");
            Inplace::empty(this);
            errors::Z_EGENERIC
        }
    }
}

/// The options for `zc_liveliness_get()`
#[repr(C)]
pub struct zc_liveliness_get_options_t {
    timeout_ms: u32,
}

/// Constructs default value `zc_liveliness_get_options_t`.
#[no_mangle]
pub extern "C" fn zc_liveliness_get_options_default(this: &mut zc_liveliness_get_options_t) {
    *this = zc_liveliness_get_options_t { timeout_ms: 10000 };
}

/// Queries liveliness tokens currently on the network with a key expression intersecting with `key_expr`.
///
/// @param session: The Zenoh session.
/// @param key_expr: The key expression to query liveliness tokens for.
/// @param callback: The callback function that will be called for each received reply.
/// @param options: Additional options for the liveliness get operation.
#[no_mangle]
pub extern "C" fn zc_liveliness_get(
    session: &z_loaned_session_t,
    key_expr: &z_loaned_keyexpr_t,
    callback: z_moved_closure_reply_t,
    options: Option<&mut zc_liveliness_get_options_t>,
) -> errors::z_error_t {
    let session = session.transmute_ref();
    let key_expr = key_expr.transmute_ref();
    let callback = core::mem::replace(callback, z_owned_closure_reply_t::empty());
    let liveliness: Liveliness<'static> = session.liveliness();
    let mut builder = liveliness.get(key_expr).callback(move |response| {
        z_closure_reply_call(z_closure_reply_loan(&callback), response.transmute_handle())
    });
    if let Some(options) = options {
        builder = builder.timeout(core::time::Duration::from_millis(options.timeout_ms as u64));
    }
    match builder.wait() {
        Ok(()) => errors::Z_OK,
        Err(e) => {
            log::error!("Failed to subscribe to liveliness: {e}");
            errors::Z_EGENERIC
        }
    }
}
