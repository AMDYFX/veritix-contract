#![no_std]
#![allow(dead_code)]
#![allow(deprecated)]

mod admin;
mod admin_test;
mod allowance;
mod allowance_test;
mod balance;
mod balance_test;
mod batch;
mod batch_test;
mod boundary_test;
mod contract;
mod dispute;
mod dispute_test;
mod divi;
mod dividend_test;
mod escrow;
mod escrow_test;
#[cfg(test)]
mod event_test;
mod freeze;
mod freeze_test;
mod multi_escrow;
mod multi_escrow_test;
mod pause;
mod pause_test;
mod permit;
mod permit_test;
mod recurring;
mod recurring_test;
#[cfg(test)]
mod sep41_test;
mod snapshot;
mod snapshot_test;
mod splitter;
mod splitter_test;
mod storage_types;
#[cfg(test)]
mod test;
mod validation;
mod version_test;
mod whitelist;
