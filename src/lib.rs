#![no_std]
// The contract is being rebuilt module by module, so a few storage helpers
// land ahead of the contract entry points that will eventually call them.
#![allow(dead_code)]

// The host-side tests build expected event topic lists with `std::vec!` and
// catch the panics that guards raise. The crate is `no_std` so the contract
// never links std, but the test build does.
#[cfg(test)]
extern crate std;

// `soroban-sdk` supplies the wasm `#[panic_handler]`, so this crate must not
// declare a second one — two handlers in one link are a duplicate `panic_impl`
// lang item. But a handler is mandatory for a `no_std` wasm module, and the
// SDK's is only linked once something in the crate actually references it. The
// crate root is empty until the first module lands, so reference the SDK here
// to keep its panic handler present at every point in the build. This is an
// anonymous import: it pulls the dependency in without binding a name, and
// introduces no unused-import warning.
use soroban_sdk as _;

// The SDK's handler is gated on `target_family = "wasm"`, so the host library
// target still has no handler at all. A `#![no_std]` crate built for a host
// target therefore cannot be built directly, which is why tooling lints the
// contract with `cargo clippy --lib --target wasm32v1-none` and lints the tests
// with `cargo clippy --tests` on the host, where `extern crate std` above
// supplies a handler. See .github/workflows/ci.yml.

// Module declarations are added here as each module lands in the backlog, so
// that every link in the stack compiles on its own. Test modules stay behind
// `#[cfg(test)]` and are never compiled into the wasm artifact.
//
// `pub mod`, per CONTRIBUTING.md. It also keeps `dead_code` honest: helpers
// land here before the modules that call them, and a private module would
// report them as unused the moment they are added. On a `cdylib` contract
// crate `pub` does not widen the wasm export surface — only `#[contractimpl]`
// entry points are callable on-chain.
pub mod admin;
pub mod airdrop;
pub mod allowance;
pub mod authorization;
pub mod balance;
pub mod batch;
pub mod contract;
pub mod control;
pub mod dispute;
pub mod dispute_stats;
pub mod dividend;
pub mod emergency;
pub mod escrow;
pub mod events;
pub mod fee;
pub mod info;
pub mod metadata;
pub mod multi_escrow;
pub mod recurring;
pub mod splitter;
pub mod storage_types;
pub mod validation;
pub mod whitelist;
pub mod whitelist_batch;
pub mod whitelist_signed;

#[cfg(test)]
mod admin_test;
#[cfg(test)]
mod allowance_test;
#[cfg(test)]
mod dispute_test;
#[cfg(test)]
mod event_test;
#[cfg(test)]
mod multi_escrow_test;
#[cfg(test)]
mod sep41_test;
