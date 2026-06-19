mod machine;
mod memory;
mod vm;
mod trace;
mod debug;
mod debugger_error;

use core_utils::filesystem::*;
use core_utils::debug::*;
use core_utils::log::init_logger;
use riscv_asm_lib::r5asm::assembler::*;

use crate::machine::machine::Machine;

use log::{info, error};

use log::LevelFilter;
use log4rs::{
    append::file::FileAppender,
    config::{Appender, Config, Root},
    encode::pattern::PatternEncoder,
};

use std::env;
use std::path::PathBuf;

fn init_debugger_logger(log_file: &str) {
    let logfile = FileAppender::builder()
        .encoder(Box::new(
            PatternEncoder::new(
                "{d(%Y-%m-%d %H:%M:%S)} [{l}] {m}{n}"
            )
        ))
        .build(log_file)
        .unwrap();

    let config = Config::builder()
        .appender(
            Appender::builder()
                .build("file", Box::new(logfile))
        )
        .build(
            Root::builder()
                .appender("file")
                .build(LevelFilter::Debug)
        )
        .unwrap();

    log4rs::init_config(config).unwrap();
}

fn get_log_file() -> String {
    let args: Vec<String> = env::args().collect();

    // 1. CLI argument: --log-file <path>
    if let Some(path) = args
        .windows(2)
        .find(|w| w[0] == "--log-file")
        .map(|w| &w[1])
    {
        return path.clone();
    }

    // 2. fallback: executable directory
    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join("riscv_debugger.log");
            return p.to_string_lossy().to_string();
        }
    }

    // 3. final fallback: current directory
    let fallback = env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("riscv_debugger.log");

    fallback.to_string_lossy().to_string()
}

fn main() {
    let log_file = get_log_file();

    init_debugger_logger(&log_file);

    log::info!("debugger starting");

    std::thread::sleep(
        std::time::Duration::from_secs(30)
    );
}
