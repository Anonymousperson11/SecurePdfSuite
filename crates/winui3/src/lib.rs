//! Pure native `WinUI 3` bindings, bootstrapper, and ergonomic `Fluent` UI abstractions.

/// Raw generated `WinUI 3` and `Win32` bindings.
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

pub mod app;
pub mod bootstrap;
pub mod controls;
pub mod dialog;
pub mod interop;
pub mod window;
