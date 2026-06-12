#![allow(warnings)]

use std::{env, fs};  
use copy_to_output::copy_to_output;

fn main() {
    let build_type = &env::var("PROFILE").unwrap();

    println!("cargo:return-if-changed=tests/code.asm");
    copy_to_output("tests/code.asm", build_type).expect("Failed to copy code.asm to output directory");
}