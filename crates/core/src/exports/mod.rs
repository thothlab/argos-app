//! Export adapters that turn an Argos workspace tree (or a single run)
//! into third-party formats — the inverse of [`crate::imports`].
//!
//! Each adapter is a pure function over the in-memory representation;
//! the host shell decides where the resulting bytes land (file,
//! clipboard, stdout).

#![allow(missing_docs)]

pub mod har;
pub mod postman;
