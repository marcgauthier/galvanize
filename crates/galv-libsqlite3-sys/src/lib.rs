#![expect(non_snake_case, non_camel_case_types)]
#![cfg_attr(not(test), no_std)]
// force linking to openssl
#[cfg(feature = "bundled-sqlcipher-vendored-openssl")]
extern crate openssl_sys;

pub use self::error::*;

use core::mem;

mod error;

#[must_use]
pub fn SQLITE_STATIC() -> sqlite3_destructor_type {
    None
}

#[must_use]
pub fn SQLITE_TRANSIENT() -> sqlite3_destructor_type {
    Some(unsafe { mem::transmute::<isize, unsafe extern "C" fn(*mut core::ffi::c_void)>(-1_isize) })
}

#[allow(dead_code, clippy::all)]
mod bindings {
    include!(concat!(env!("OUT_DIR"), "/bindgen.rs"));
}
pub use bindings::*;

impl Default for sqlite3_vtab {
    fn default() -> Self {
        unsafe { mem::zeroed() }
    }
}

impl Default for sqlite3_vtab_cursor {
    fn default() -> Self {
        unsafe { mem::zeroed() }
    }
}

extern "C" {
    pub fn sqlite3_key(
        db: *mut sqlite3,
        pKey: *const ::core::ffi::c_void,
        nKey: ::core::ffi::c_int,
    ) -> ::core::ffi::c_int;

    pub fn sqlite3_key_v2(
        db: *mut sqlite3,
        zDb: *const ::core::ffi::c_char,
        pKey: *const ::core::ffi::c_void,
        nKey: ::core::ffi::c_int,
    ) -> ::core::ffi::c_int;

    pub fn sqlite3_rekey(
        db: *mut sqlite3,
        pKey: *const ::core::ffi::c_void,
        nKey: ::core::ffi::c_int,
    ) -> ::core::ffi::c_int;

    pub fn sqlite3_rekey_v2(
        db: *mut sqlite3,
        zDb: *const ::core::ffi::c_char,
        pKey: *const ::core::ffi::c_void,
        nKey: ::core::ffi::c_int,
    ) -> ::core::ffi::c_int;

    pub fn sqlite3mc_config(
        db: *mut sqlite3,
        paramName: *const ::core::ffi::c_char,
        ...
    ) -> ::core::ffi::c_int;
}

