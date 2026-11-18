//! Self-extracting `LZMS` installer and packer library for `SecurePdfCreator` and `SecurePdfReader`.

/// Raw generated Win32 compression, CNG, registry, and shell bindings.
#[allow(
    dead_code,
    elided_lifetimes_in_paths,
    missing_docs,
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    rust_2018_idioms,
    trivial_casts,
    trivial_numeric_casts,
    unsafe_op_in_unsafe_fn,
    unused_qualifications,
    warnings,
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::cargo,
    clippy::restriction
)]
pub mod bindings;

pub mod cab;
pub mod logging;
pub mod package;
pub mod paths;
pub mod registry;
pub mod selftest;
pub mod uninstall;
pub mod win32_install;
