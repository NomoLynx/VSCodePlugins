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

use dap::prelude::*;
use dap::events::*;
use dap::requests::*;
use dap::responses::*;

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


fn main() -> Result<(), Box<dyn std::error::Error>> {
    let log_file = get_log_file();
    init_debugger_logger(&log_file);

    let stdin = std::io::stdin();
    let stdout = std::io::stdout();

    let reader = std::io::BufReader::new(stdin);
    let writer = std::io::BufWriter::new(stdout);

    let mut server = Server::new(reader, writer);

    log::info!("debugger started");

    loop {
        let req = match server.poll_request()? {
            Some(req) => req,
            None => break,
        };

        log::info!("received request: {:?}", req.command);

        match &req.command {
            Command::Initialize(args) => {
                log::info!("initialize");
                let rsp = ResponseBody::Initialize(
                    types::Capabilities {
                        supports_configuration_done_request: Some(true),
                        ..Default::default()
                    }
                );
                server.respond(req.success(rsp))?;

                // 🔥 REQUIRED: tell VS Code debugger is ready
                server.send_event(Event::Initialized)?;
            }
            Command::Launch(args) => {
                log::info!("launch: {:?}", args);

                // start the simulator, load the program, etc.

                server.respond(req.success(ResponseBody::Launch))?;

                // 🔥 REQUIRED: simulate "program paused"
                server.send_event(Event::Stopped( 
                        StoppedEventBody {
                        reason: types::StoppedEventReason::Entry,
                        description: Some("entry".into()),
                        thread_id: Some(1),
                        preserve_focus_hint: Some(true),
                        text: None,
                        all_threads_stopped: Some(true),
                        hit_breakpoint_ids: Some(vec![]),
                    } 
                ))?;
            }
            Command::Threads => {
                server.respond(req.success(
                    ResponseBody::Threads(ThreadsResponse {
                        threads: vec![
                            types::Thread {
                                id: 1,
                                name: "hart0".to_string(),
                            }
                        ]
                    })
                ))?;
            }
            Command::StackTrace(args) => {
                server.respond(req.success(
                    ResponseBody::StackTrace(StackTraceResponse {
                        stack_frames: vec![
                            types::StackFrame {
                                id: 1,
                                name: "main".to_string(),
                                line: 1,
                                column: 1,
                                source: None,
                                ..Default::default()
                            }
                        ],
                        total_frames: Some(1),
                    })
                ))?;
            }
            Command::Disconnect(_) => {
                log::info!("disconnect");
                server.respond(req.success(ResponseBody::Disconnect))?;
                break;
            }
            other => {
                log::info!("unhandled request: {:?}", other);
                server.respond(req.success(ResponseBody::Terminate))?;
            }
        }
    }

    Ok(())
}
