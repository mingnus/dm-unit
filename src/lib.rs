#![feature(map_first_last)]

extern crate clap;
extern crate elf;
extern crate log;
extern crate nom;
extern crate regex;
extern crate thiserror;

pub mod block_manager;
pub mod decode;
pub mod fixture;
pub mod guest;
pub mod loader;
pub mod memory;
pub mod primitive;
pub mod stubs;
pub mod test_runner;
pub mod tests;
pub mod user_data;
pub mod vm;
pub mod wrappers;
