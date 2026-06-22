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

fn to_file_uri(path: &str) -> String {
    let p = std::path::Path::new(path)
        .canonicalize()
        .unwrap_or_else(|_| std::path::PathBuf::from(path));

    let p = p.to_string_lossy().replace("\\", "/");

    format!("file:///{}", p)
}

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

    // program_path used for stack trace source reference, set during launch request
    let mut program_path: Option<String> = None; 

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

                // keep file path for later use in stack trace
                program_path = args
                        .additional_data
                        .as_ref()
                        .and_then(|v| v["program"].as_str())
                        .map(|s| s.to_string());
                log::info!("program path: {:?}", program_path);

                // start the simulator, load the program, etc.

                server.respond(req.success(ResponseBody::Launch))?;
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
                log::info!("stackTrace: {:?}", args);

                let file_name = program_path
                                                .as_ref()
                                                .and_then(|p| std::path::Path::new(p).file_name())
                                                .and_then(|n| n.to_str())
                                                .unwrap_or("program.rv.s")
                                                .to_string();
                let file_path = to_file_uri(program_path.as_ref().unwrap_or(&"".to_string()));
                log::info!("stackTrace: {file_name:?} and file_path = {file_path}");

                log::info!("stack frame path = {:?}", program_path);
                log::info!("exists = {}", std::path::Path::new(program_path.as_ref().unwrap()).exists());

                server.respond(req.success(
                    ResponseBody::StackTrace(StackTraceResponse {
                        stack_frames: vec![
                            types::StackFrame {
                                id: 1,
                                name: "main".to_string(),
                                line: 1,
                                column: 1,
                                source: Some(types::Source {
                                    name: Some(file_name),
                                    path: program_path.clone(),
                                    ..Default::default()
                                }),
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
            Command::ConfigurationDone => {
                log::info!("configurationDone");

                server.respond(req.success(ResponseBody::ConfigurationDone))?;

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
            other => {
                log::error!("Unhandled request: {:?}", other);
                server.respond(req.success(ResponseBody::Terminate))?;
            }
        }
    }

    Ok(())
}
