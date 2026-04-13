/// Public library surface for `tokenizer-svc`.
///
/// Integration tests import this crate directly; the `[[bin]]` target
/// delegates all real logic here.
pub mod config;
pub mod error;
pub mod handlers;
pub mod metrics;
pub mod routes;
pub mod state;
pub mod tokenizer;
