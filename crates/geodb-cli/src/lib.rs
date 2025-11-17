//! geodb-cli
//! ==========
//!
//! Command-line interface for the `geodb-core` geographic database.
//!
//! This crate primarily provides a binary (`geodb-cli`). We include a small
//! library target so that docs.rs renders a documentation page and shows this
//! overview. See the README for full usage examples.
//!
//! Quick start
//! -----------
//!
//! Install the CLI from crates.io:
//!
//! ```text
//! cargo install geodb-cli
//! ```
//!
//! Basic usage:
//!
//! ```text
//! geodb-cli --help
//! geodb-cli stats
//! geodb-cli find-country US
//! geodb-cli list-cities --country US --state CA
//! ```
//!
//! For programmatic access to the data structures and APIs, use the
//! [`geodb-core`] crate directly.
//!
//! Links
//! -----
//! - Repository: <https://github.com/holg/geodb-rs>
//! - Core crate: <https://docs.rs/geodb-core>
//!
#![cfg_attr(docsrs, feature(doc_cfg))]

// This library target intentionally exposes no API; the binary is the primary
// deliverable. The presence of this file enables a rendered page on docs.rs.
