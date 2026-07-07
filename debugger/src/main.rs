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

use log::{info, warn, error};

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
use dap::types;

use std::env;
use std::path::PathBuf;
use std::collections::HashMap;

#[cfg(windows)]
mod win32 {
    extern "system" {
        pub fn PeekNamedPipe(
            hNamedPipe: isize,
            lpBuffer: *mut u8,
            nBufferSize: u32,
            lpBytesRead: *mut u32,
            lpTotalBytesAvail: *mut u32,
            lpBytesLeftThisMessage: *mut u32,
        ) -> i32;
    }
}

/// Check if stdin (piped from VSCode) has pending data without blocking.
/// Used during Continue to detect Pause/Terminate requests.
#[cfg(windows)]
fn stdin_has_data() -> bool {
    use std::os::windows::io::AsRawHandle;
    unsafe {
        let handle = std::io::stdin().as_raw_handle() as isize;
        let mut total_avail: u32 = 0;
        win32::PeekNamedPipe(
            handle,
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            &mut total_avail,
            std::ptr::null_mut(),
        ) != 0
            && total_avail > 0
    }
}

#[cfg(not(windows))]
fn stdin_has_data() -> bool {
    use std::os::unix::io::AsRawFd;
    use std::os::raw::{c_int, c_short, c_ulong};

    #[repr(C)]
    struct PollFd {
        fd: c_int,
        events: c_short,
        revents: c_short,
    }

    const POLLIN: c_short = 0x001;

    extern "C" {
        fn poll(fds: *mut PollFd, nfds: c_ulong, timeout: c_int) -> c_int;
    }

    let mut pfd = PollFd {
        fd: std::io::stdin().as_raw_fd(),
        events: POLLIN,
        revents: 0,
    };

    unsafe { poll(&mut pfd, 1, 0) > 0 }
}

/// Variables reference base for register scopes.
/// We use different variables_reference values to distinguish scopes:
/// - 1000: integer registers (x0-x31)
/// - 2000: float registers (f0-f31)
/// Register variables themselves use their index as reference (no children).
const INT_REG_BASE: i64 = 1000;
const FP_REG_BASE: i64 = 2000;

/// RISC-V integer register names
const INT_REG_NAMES: [&str; 32] = [
    "zero", "ra", "sp", "gp", "tp", "t0", "t1", "t2",
    "s0/fp", "s1", "a0", "a1", "a2", "a3", "a4", "a5",
    "a6", "a7", "s2", "s3", "s4", "s5", "s6", "s7",
    "s8", "s9", "s10", "s11", "t3", "t4", "t5", "t6",
];

fn init_debugger_logger(log_file: &str) {
    // Try to initialize file-based logger; fall back to silent if anything fails.
    // Don't unwrap — logging failure must not crash the debugger.
    let logfile = match FileAppender::builder()
        .encoder(Box::new(
            PatternEncoder::new(
                "{d(%Y-%m-%d %H:%M:%S)} [{l}] {m}{n}"
            )
        ))
        .build(log_file)
    {
        Ok(f) => f,
        Err(e) => {
            eprintln!("[debugger] WARNING: failed to create log file '{}': {}. Logging disabled.", log_file, e);
            return;
        }
    };

    let config = match Config::builder()
        .appender(
            Appender::builder()
                .build("file", Box::new(logfile))
        )
        .build(
            Root::builder()
                .appender("file")
                .build(LevelFilter::Debug)
        )
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[debugger] WARNING: failed to build log config: {}. Logging disabled.", e);
            return;
        }
    };

    if let Err(e) = log4rs::init_config(config) {
        eprintln!("[debugger] WARNING: failed to init log4rs: {}. Logging disabled.", e);
    }
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
    let mut machine: Option<Machine> = None;
    let mut current_line: usize = 1;
    let mut offset_to_line: HashMap<usize, usize> = HashMap::new();
    let mut breakpoints: std::collections::HashMap<i64, i64> = std::collections::HashMap::new();
    /// Monotonically increasing frame ID — VS Code uses this to detect that
    /// the stack has changed and will re-request Scopes/Variables.
    let mut frame_id_counter: i64 = 0;

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
                        supports_set_variable: Some(true),
                        supports_goto_targets_request: Some(true),
                        supports_restart_frame: Some(true),
                        supports_terminate_request: Some(true),
                        supports_breakpoint_locations_request: Some(true),
                        supports_delayed_stack_trace_loading: Some(false),
                        exception_breakpoint_filters: Some(vec![]),
                        ..Default::default()
                    }
                );
                server.respond(req.success(rsp))?;

                // 🔥 REQUIRED: tell VS Code debugger is ready
                server.send_event(Event::Initialized)?;
            }
            Command::BreakpointLocations(args) => {
                log::info!("breakpointLocations: line={}", args.line);
                log::info!("breakpointLocations: offset_to_line={:?}", offset_to_line);

                let mut locations = Vec::new();
                let target_line = args.line as usize;

                // Check if this line has an instruction (exists in our offset_to_line mapping)
                let has_instruction = offset_to_line.values().any(|&line| line == target_line);
                
                // If offset_to_line is empty (program not loaded yet), 
                // still report the line as breakpoint-able so VSCode shows the breakpoint UI
                if has_instruction || offset_to_line.is_empty() {
                    locations.push(types::BreakpointLocation {
                        line: args.line,
                        column: Some(1),
                        end_line: None,
                        end_column: None,
                    });
                }

                log::info!("breakpointLocations: returning {} locations (has_instruction={})", 
                    locations.len(), has_instruction);

                server.respond(req.success(
                    ResponseBody::BreakpointLocations(BreakpointLocationsResponse {
                        breakpoints: locations,
                    })
                ))?;
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

                // load and assemble the .rv.s file
                if let Some(ref path) = program_path {
                    match load_and_assemble(path, &mut offset_to_line) {
                        Ok(m) => {
                            // Set initial line from hart 0's PC
                            if let Some(hart) = m.get_hart(0) {
                                if let Some(line) = offset_to_line.get(&hart.pc) {
                                    current_line = *line;
                                } else {
                                    current_line = 1;
                                }
                            } else {
                                current_line = 1;
                            }
                            machine = Some(m);
                            log::info!("program loaded successfully, current_line={}", current_line);
                        }
                        Err(e) => {
                            log::error!("failed to load program: {}", e);
                            let err_msg = format!("Failed to load program: {}\r\n", e);
                            server.send_event(Event::Output(OutputEventBody {
                                category: Some(types::OutputEventCategory::Stderr),
                                output: err_msg,
                                ..Default::default()
                            }))?;
                            let err_msg = format!("Failed to load program: {}", e);
                            server.respond(req.error(&err_msg))?;
                            continue;
                        }
                    }
                }

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
                log::info!("stackTrace: file_name = {file_name:?}");

                log::info!("stack frame path = {:?}", program_path);

                // Bump frame id so VS Code knows this is a new stop and
                // will re-request Scopes/Variables rather than reusing cached data.
                frame_id_counter += 1;

                let sf = types::StackFrame {
                    id: frame_id_counter,
                    name: "main".to_string(),
                    line: current_line as i64,
                    column: 1,
                    end_line: Some(current_line as i64),
                    end_column: Some(100),
                    source: Some(types::Source {
                        name: Some(file_name),
                        path: program_path.clone(),
                        presentation_hint: Some(types::PresentationHint::Normal),
                        ..Default::default()
                    }),
                    ..Default::default()
                };
                log::info!("stack frame (id={}): {:?}", frame_id_counter, sf);
                server.respond(req.success(
                    ResponseBody::StackTrace(StackTraceResponse {
                        stack_frames: vec![sf],
                        total_frames: Some(1),
                    })
                ))?;
            }
            Command::StepIn(args) => {
                log::info!("STEP IN");
                let outcome = do_single_step(&mut machine, &mut current_line, &offset_to_line);

                if let StepOutcome::Terminated = outcome {
                    server.respond(req.success(ResponseBody::StepIn))?;
                    server.send_event(Event::Exited(ExitedEventBody { exit_code: 0 }))?;
                    server.send_event(Event::Terminated(None))?;
                    break;
                }

                let hit_bp_id = breakpoints.get(&(current_line as i64)).copied();
                let stop_reason = if hit_bp_id.is_some() { types::StoppedEventReason::Breakpoint } else { types::StoppedEventReason::Step };
                let hit_ids: Vec<i64> = if let Some(id) = hit_bp_id { vec![id] } else { vec![] };
                let desc = if hit_bp_id.is_some() { "breakpoint hit" } else { "step in" };

                // Send StoppedEvent BEFORE response so VSCode enters stopped state
                // and properly requests Scopes/Variables for updated register display.
                server.send_event(Event::Stopped(StoppedEventBody {
                    reason: stop_reason,
                    description: Some(desc.into()),
                    thread_id: Some(1),
                    preserve_focus_hint: Some(false),
                    text: None,
                    all_threads_stopped: Some(true),
                    hit_breakpoint_ids: Some(hit_ids),
                }))?;

                server.respond(req.success(ResponseBody::StepIn))?;
            }
            Command::Next(args) => {
                log::info!("STEP OVER");
                let outcome = do_single_step(&mut machine, &mut current_line, &offset_to_line);

                if let StepOutcome::Terminated = outcome {
                    server.respond(req.success(ResponseBody::Next))?;
                    server.send_event(Event::Exited(ExitedEventBody { exit_code: 0 }))?;
                    server.send_event(Event::Terminated(None))?;
                    break;
                }

                let hit_bp_id = breakpoints.get(&(current_line as i64)).copied();
                let stop_reason = if hit_bp_id.is_some() { types::StoppedEventReason::Breakpoint } else { types::StoppedEventReason::Step };
                let hit_ids: Vec<i64> = if let Some(id) = hit_bp_id { vec![id] } else { vec![] };
                let desc = if hit_bp_id.is_some() { "breakpoint hit" } else { "step next" };

                // Send StoppedEvent BEFORE response so VSCode enters stopped state
                // and properly requests Scopes/Variables for updated register display.
                server.send_event(Event::Stopped(StoppedEventBody {
                    reason: stop_reason,
                    description: Some(desc.into()),
                    thread_id: Some(1),
                    preserve_focus_hint: Some(false),
                    text: None,
                    all_threads_stopped: Some(true),
                    hit_breakpoint_ids: Some(hit_ids),
                }))?;

                server.respond(req.success(ResponseBody::Next))?;
            }
            Command::Continue(_args) => {
                log::info!("CONTINUE");

                let mut should_terminate = false;
                let mut hit_breakpoint = false;
                let mut hit_bp_id: Option<i64> = None;
                let mut steps = 0;
                let mut paused_for_dap = false;
                const DAP_POLL_INTERVAL: usize = 1000;

                if let Some(ref mut m) = machine {
                    loop {
                        match m.step_hart(0) {
                            Ok(()) => {
                                steps += 1;
                                // Check if we've run past the end
                                if let Some(hart) = m.get_hart(0) {
                                    if let Some(line) = offset_to_line.get(&hart.pc) {
                                        current_line = *line;
                                        // Check if we hit a breakpoint
                                        if let Some(&bp_id) = breakpoints.get(&(current_line as i64)) {
                                            hit_breakpoint = true;
                                            hit_bp_id = Some(bp_id);
                                            log::info!("breakpoint hit at line {} (id={}) after {} steps", current_line, bp_id, steps);
                                            log::info!("at breakpoint: t1(x6)={:x}, t2(x7)={:x}, t0(x5)={:x}, a0(x10)={:x}, a1(x11)={:x}, a2(x12)={:x}, a3(x13)={:x}",
                                                hart.x.read(6), hart.x.read(7), hart.x.read(5),
                                                hart.x.read(10), hart.x.read(11), hart.x.read(12),
                                                hart.x.read(13));
                                            break;
                                        }
                                    } else {
                                        should_terminate = true;
                                        log::info!("program finished after {} steps", steps);
                                        break;
                                    }
                                }
                                // Poll for pending DAP requests (Pause/Terminate) every N steps
                                if steps % DAP_POLL_INTERVAL == 0 && stdin_has_data() {
                                    log::info!("continue: DAP data pending at step {}, yielding", steps);
                                    paused_for_dap = true;
                                    break;
                                }
                                // safety: max 100000 steps to avoid infinite loops
                                if steps > 100000 {
                                    log::warn!("continue reached max steps limit");
                                    break;
                                }
                            }
                            Err(e) => {
                                log::info!("continue error at step {}: {:?}", steps, e);
                                should_terminate = true;
                                break;
                            }
                        }
                    }
                }

                if paused_for_dap {
                    // Yield to outer DAP loop: respond to Continue so VSCode stays in
                    // "running" state. The pending Pause/Terminate will be picked up
                    // by the next poll_request() iteration.
                    server.respond(req.success(ResponseBody::Continue(
                        ContinueResponse {
                            all_threads_continued: Some(true),
                        }
                    )))?;
                } else if should_terminate {
                    server.respond(req.success(ResponseBody::Continue(
                        ContinueResponse {
                            all_threads_continued: Some(true),
                        }
                    )))?;
                    server.send_event(Event::Exited(ExitedEventBody {
                        exit_code: 0,
                    }))?;
                    server.send_event(Event::Terminated(None))?;
                    break;
                } else {
                    // Stopped at breakpoint or step limit
                    // Send StoppedEvent BEFORE ContinueResponse so VSCode properly
                    // enters the stopped state and requests Scopes/Variables.
                    let stop_reason = if hit_breakpoint {
                        types::StoppedEventReason::Breakpoint
                    } else {
                        types::StoppedEventReason::Pause
                    };
                    let desc = if hit_breakpoint {
                        format!("breakpoint hit at line {}", current_line)
                    } else {
                        "paused (step limit reached)".into()
                    };
                    let hit_ids: Vec<i64> = if let Some(id) = hit_bp_id { vec![id] } else { vec![] };
                    server.send_event(Event::Stopped(StoppedEventBody {
                        reason: stop_reason,
                        description: Some(desc),
                        thread_id: Some(1),
                        preserve_focus_hint: Some(false),
                        text: None,
                        all_threads_stopped: Some(true),
                        hit_breakpoint_ids: Some(hit_ids),
                    }))?;

                    // Respond to Continue AFTER StoppedEvent — stream ordering guarantees
                    // VSCode receives the StoppedEvent first.
                    server.respond(req.success(ResponseBody::Continue(
                        ContinueResponse {
                            all_threads_continued: None,
                        }
                    )))?;
                }
            }
            Command::Scopes(args) => {
                log::info!("scopes request: frame_id={}", args.frame_id);

                server.respond(req.success(
                    ResponseBody::Scopes(ScopesResponse {
                        scopes: vec![
                            types::Scope {
                                name: "Registers".to_string(),
                                presentation_hint: Some(types::ScopePresentationhint::Registers),
                                variables_reference: INT_REG_BASE,
                                named_variables: Some(32),
                                indexed_variables: None,
                                expensive: false,
                                source: None,
                                line: None,
                                column: None,
                                end_line: None,
                                end_column: None,
                            },
                            types::Scope {
                                name: "Float Registers".to_string(),
                                presentation_hint: Some(types::ScopePresentationhint::Registers),
                                variables_reference: FP_REG_BASE,
                                named_variables: Some(32),
                                indexed_variables: None,
                                expensive: false,
                                source: None,
                                line: None,
                                column: None,
                                end_line: None,
                                end_column: None,
                            },
                        ],
                    })
                ))?;
            }
            Command::Variables(args) => {
                log::info!("variables request: ref={}", args.variables_reference);
                log::info!("machine is_some={}", machine.is_some());

                let mut variables: Vec<types::Variable> = Vec::new();

                if args.variables_reference == INT_REG_BASE {
                    // Return all integer registers
                    if let Some(ref m) = machine {
                        if let Some(hart) = m.get_hart(0) {
                            for i in 0..32 {
                                let abi_name = INT_REG_NAMES[i];
                                let name = format!("x{}/{}", i, abi_name);
                                let value = hart.x.read(i);
                                variables.push(types::Variable {
                                    name,
                                    value: format!("0x{:016x}", value),
                                    type_field: Some("u64".to_string()),
                                    evaluate_name: Some(format!("x{}", i)),
                                    variables_reference: 0,
                                    named_variables: None,
                                    indexed_variables: None,
                                    presentation_hint: None,
                                    memory_reference: None,
                                });
                            }
                        }
                    }
                } else if args.variables_reference == FP_REG_BASE {
                    // Return all float registers
                    if let Some(ref m) = machine {
                        if let Some(hart) = m.get_hart(0) {
                            for i in 0..32 {
                                let name = format!("f{}", i);
                                let value = hart.f.regs[i].value;
                                variables.push(types::Variable {
                                    name,
                                    value: format!("{}", value),
                                    type_field: Some("f64".to_string()),
                                    evaluate_name: Some(format!("f{}", i)),
                                    variables_reference: 0,
                                    named_variables: None,
                                    indexed_variables: None,
                                    presentation_hint: None,
                                    memory_reference: None,
                                });
                            }
                        }
                    }
                }

                server.respond(req.success(
                    ResponseBody::Variables(VariablesResponse {
                        variables,
                    })
                ))?;
            }
            Command::SetVariable(args) => {
                log::info!("setVariable: ref={}, name={}, value={}", 
                    args.variables_reference, args.name, args.value);

                let mut write_ok = false;
                let mut new_value = args.value.clone();

                if args.variables_reference == INT_REG_BASE {
                    // Parse register index from name like "x5/t0"
                    if let Some(reg_name) = args.name.split('/').next() {
                        if reg_name.starts_with('x') {
                            if let Ok(idx) = reg_name[1..].parse::<usize>() {
                                if idx < 32 {
                                    if let Ok(val) = parse_u64(&args.value) {
                                        if let Some(ref mut m) = machine {
                                            if let Some(hart) = m.get_hart_mut(0) {
                                                hart.x.write(idx, val); // IntegerRegisterFile::write guards x0
                                            }
                                        }
                                        new_value = format!("0x{:016x}", val);
                                        write_ok = true;
                                    }
                                }
                            }
                        }
                    }
                    if !write_ok {
                        server.respond(req.error("Failed to parse integer register name or value"))?;
                        continue;
                    }
                } else if args.variables_reference == FP_REG_BASE {
                    if let Some(reg_name) = args.name.split('/').next() {
                        if reg_name.starts_with('f') {
                            if let Ok(idx) = reg_name[1..].parse::<usize>() {
                                if idx < 32 {
                                    if let Ok(val) = args.value.parse::<f64>() {
                                        if let Some(ref mut m) = machine {
                                            if let Some(hart) = m.get_hart_mut(0) {
                                                hart.f.regs[idx].value = val;
                                            }
                                        }
                                        new_value = format!("{}", val);
                                        write_ok = true;
                                    }
                                }
                            }
                        }
                    }
                    if !write_ok {
                        server.respond(req.error("Failed to parse float register name or value"))?;
                        continue;
                    }
                }

                server.respond(req.success(
                    ResponseBody::SetVariable(SetVariableResponse {
                        value: new_value,
                        type_field: None,
                        variables_reference: None,
                        named_variables: None,
                        indexed_variables: None,
                    })
                ))?;
            }
            Command::GotoTargets(args) => {
                log::info!("gotoTargets: source={:?}, line={}", args.source, args.line);

                // Return all lines in the current program as goto targets
                let mut targets = Vec::new();
                if let Some(ref m) = machine {
                    if let Some(hart) = m.get_hart(0) {
                        let program_id = hart.program_id;
                        if program_id < m.programs.len() {
                            let prog = &m.programs[program_id];
                            let items = prog.get_text_section_items();
                            for (idx, item) in items.iter().enumerate() {
                                // Only include items that have an instruction and a source line
                                if item.get_inc().is_none() {
                                    continue;
                                }
                                let line = offset_to_line.get(&item.get_offset()).copied().unwrap_or(idx + 1) as i64;
                                targets.push(types::GotoTarget {
                                    id: line,
                                    label: format!("Line {}", line),
                                    line,
                                    column: Some(1),
                                    end_line: None,
                                    end_column: None,
                                    instruction_pointer_reference: Some(format!("{}", idx)),
                                });
                            }
                        }
                    }
                }

                server.respond(req.success(
                    ResponseBody::GotoTargets(GotoTargetsResponse {
                        targets,
                    })
                ))?;
            }
            Command::Goto(args) => {
                log::info!("goto: thread_id={}, target_id={}", args.thread_id, args.target_id);

                // target_id is the source line number; find offset from mapping
                let target_line = args.target_id as usize;
                let pc = offset_to_line.iter()
                    .find(|(_, &line)| line == target_line)
                    .map(|(&offset, _)| offset);

                if let Some(new_pc) = pc {
                    if let Some(ref mut m) = machine {
                        if let Some(hart) = m.get_hart_mut(0) {
                            hart.set_pc(new_pc);
                            current_line = target_line;
                            log::info!("goto: set pc to {} (line {})", new_pc, current_line);
                        }
                    }

                    server.respond(req.success(ResponseBody::Goto))?;

                    server.send_event(Event::Stopped(StoppedEventBody {
                        reason: types::StoppedEventReason::Goto,
                        description: Some(format!("jumped to line {}", current_line)),
                        thread_id: Some(1),
                        preserve_focus_hint: Some(false),
                        text: None,
                        all_threads_stopped: Some(true),
                        hit_breakpoint_ids: Some(vec![]),
                    }))?;
                } else {
                    server.respond(req.error(&format!("No code mapped to line {}", target_line)))?;
                }
            }
            Command::SetBreakpoints(args) => {
                log::info!("setBreakpoints: source.path={:?}, source.name={:?}", 
                    args.source.path, args.source.name);
                log::info!("setBreakpoints: breakpoints={:?}", args.breakpoints);
                log::info!("setBreakpoints: lines={:?}", args.lines);

                // Clear old breakpoints and set new ones
                breakpoints.clear();
                let mut bp_list = Vec::new();
                if let Some(ref source_bps) = args.breakpoints {
                    for bp in source_bps {
                        let bp_id = bp_list.len() as i64 + 1;
                        breakpoints.insert(bp.line, bp_id);
                        bp_list.push(types::Breakpoint {
                            id: Some(bp_id),
                            verified: true,
                            message: None,
                            source: Some(args.source.clone()),
                            line: Some(bp.line),
                            column: bp.column,
                            end_line: None,
                            end_column: None,
                            instruction_reference: None,
                            offset: None,
                        });
                    }
                } else if let Some(ref lines) = args.lines {
                    // Handle deprecated lines field
                    for line in lines {
                        let bp_id = bp_list.len() as i64 + 1;
                        breakpoints.insert(*line, bp_id);
                        bp_list.push(types::Breakpoint {
                            id: Some(bp_id),
                            verified: true,
                            message: None,
                            source: Some(args.source.clone()),
                            line: Some(*line),
                            column: Some(1),
                            end_line: None,
                            end_column: None,
                            instruction_reference: None,
                            offset: None,
                        });
                    }
                }
                log::info!("breakpoints set at lines: {:?}", breakpoints);
                log::info!("responding with {} breakpoints", bp_list.len());

                server.respond(req.success(
                    ResponseBody::SetBreakpoints(SetBreakpointsResponse {
                        breakpoints: bp_list,
                    })
                ))?;
            }
            Command::RestartFrame(args) => {
                log::info!("restartFrame: frame_id={}", args.frame_id);

                // Restart frame: reset PC to entry point
                if let Some(ref mut m) = machine {
                    // Re-init hart to entry point
                    match m.init_hart_to_entry_point(0) {
                        Ok(()) => {
                            if let Some(hart) = m.get_hart(0) {
                                if let Some(line) = offset_to_line.get(&hart.pc) {
                                    current_line = *line;
                                } else {
                                    current_line = 1;
                                }
                            }
                            log::info!("restartFrame: reset to line {}", current_line);
                        }
                        Err(e) => {
                            log::error!("restartFrame: failed to re-init hart: {:?}", e);
                            server.send_event(Event::Output(OutputEventBody {
                                output: format!("Restart failed: {:?}\n", e),
                                category: Some(types::OutputEventCategory::Stderr),
                                ..Default::default()
                            }))?;
                            server.respond(req.error(&format!("Restart failed: {}", e)))?;
                            continue;
                        }
                    }
                }

                server.respond(req.success(ResponseBody::RestartFrame))?;
            }
            Command::Pause(_args) => {
                log::info!("PAUSE");
                // During Continue, the simulation loop yields periodically to check
                // for pending DAP input. When Pause arrives, the simulation is already
                // at a safe stopping point. Forward the stop to VSCode.
                server.send_event(Event::Stopped(StoppedEventBody {
                    reason: types::StoppedEventReason::Pause,
                    description: Some("paused by user".into()),
                    thread_id: Some(1),
                    preserve_focus_hint: Some(false),
                    text: None,
                    all_threads_stopped: Some(true),
                    hit_breakpoint_ids: Some(vec![]),
                }))?;
                server.respond(req.ack()?)?;
            }
            Command::Disconnect(_) => {
                log::info!("disconnect");
                server.respond(req.success(ResponseBody::Disconnect))?;
                break;
            }
            Command::Terminate(_) => {
                log::info!("terminate");
                server.respond(req.success(ResponseBody::Terminate))?;
                server.send_event(Event::Exited(ExitedEventBody {
                    exit_code: 0,
                }))?;
                server.send_event(Event::Terminated(None))?;
                break;
            }
            Command::ConfigurationDone => {
                log::info!("configurationDone");

                server.respond(req.success(ResponseBody::ConfigurationDone))?;

                // If no breakpoints were set by VSCode, automatically set breakpoints
                // at all source lines that have instructions. This ensures the user
                // can step through the program even if VSCode doesn't send setBreakpoints.
                if breakpoints.is_empty() && !offset_to_line.is_empty() {
                    log::info!("no breakpoints set by VSCode, auto-setting breakpoints on all instruction lines");
                    let mut auto_id = 1;
                    for &line in offset_to_line.values() {
                        breakpoints.insert(line as i64, auto_id);
                        auto_id += 1;
                    }
                    log::info!("auto-set breakpoints at lines: {:?}", breakpoints);
                }

                // Respond to ConfigurationDone BEFORE sending StoppedEvent — stream ordering
                // guarantees VSCode processes the response first, then the event.
                server.send_event(Event::Stopped(
                    StoppedEventBody {
                        reason: types::StoppedEventReason::Breakpoint,
                        description: Some("entry".into()),
                        thread_id: Some(1),
                        preserve_focus_hint: Some(false),
                        text: None,
                        all_threads_stopped: Some(true),
                        hit_breakpoint_ids: Some(vec![]),
                    }
                ))?;
            }
            other => {
                log::error!("Unhandled request: {:?}", other);
                // Don't terminate on unhandled request - just respond with empty success
                server.respond(req.ack()?)?;
            }
        }
    }

    Ok(())
}

/// Parse a u64 from a string that may be decimal or hex (0x prefix)
fn parse_u64(s: &str) -> Result<u64, std::num::ParseIntError> {
    let s = s.trim();
    if s.starts_with("0x") || s.starts_with("0X") {
        u64::from_str_radix(&s[2..], 16)
    } else {
        s.parse::<u64>()
    }
}

/// Result of performing a single instruction step
enum StepOutcome {
    /// Stepped to next instruction, current_line already updated
    Ok,
    /// Program ended (PC out of range or step error)
    Terminated,
}

/// Execute one instruction on hart 0 and update current_line.
/// Shared by StepIn and Next — both do the same thing for RISC-V assembly.
fn do_single_step(
    machine: &mut Option<Machine>,
    current_line: &mut usize,
    offset_to_line: &HashMap<usize, usize>,
) -> StepOutcome {
    if let Some(ref mut m) = machine {
        match m.step_hart(0) {
            Ok(()) => {
                if let Some(hart) = m.get_hart(0) {
                    log::info!("after step: t1(x6)={:x}, t2(x7)={:x}, t0(x5)={:x}, a0(x10)={:x}, a1(x11)={:x}, a2(x12)={:x}, pc={}",
                        hart.x.read(6), hart.x.read(7), hart.x.read(5),
                        hart.x.read(10), hart.x.read(11), hart.x.read(12),
                        hart.pc);
                    if let Some(line) = offset_to_line.get(&hart.pc) {
                        *current_line = *line;
                        return StepOutcome::Ok;
                    } else {
                        log::info!("program finished (pc {} out of range)", hart.pc);
                    }
                }
            }
            Err(e) => {
                log::info!("step error: {:?}", e);
            }
        }
    }
    StepOutcome::Terminated
}

/// Load and assemble a .rv.s file into a Machine and build offset→line mapping
fn load_and_assemble(file_path: &str, offset_to_line: &mut HashMap<usize, usize>) -> Result<Machine, Box<dyn std::error::Error>> {
    log::info!("loading file: {}", file_path);

    let content = std::fs::read_to_string(file_path)
        .map_err(|e| format!("failed to read file {}: {}", file_path, e))?;

    log::info!("file content read, {} bytes", content.len());

    let program = parse_asm_use_default_config(&content)
        .map_err(|e| format!("assembly failed: {:?}", e))?;

    log::info!("assembly successful, {} text section items", 
        program.get_text_section_items().len());

    // Build offset → source line number mapping from instruction positions
    offset_to_line.clear();
    for item in program.get_text_section_items() {
        let offset = item.get_offset();
        if let Some(inst) = item.get_inc() {
            // Try to get the source line from the instruction's name location
            if let Some(range) = inst.get_name_location() {
                // SourceRange uses 1-based line numbers
                let line = range.start.line as usize;
                offset_to_line.insert(offset, line);
                log::info!("  offset {} -> source line {}", offset, line);
            }
        }
    }
    log::info!("offset_to_line mapping: {:?}", offset_to_line);

    let mut machine = Machine::new();
    let program_id = machine.add_program(program)
        .map_err(|e| format!("failed to add program: {:?}", e))?;

    // assign program to hart 0
    if let Some(hart) = machine.get_hart_mut(0) {
        hart.program_id = program_id;
    }

    // init hart to entry point
    machine.init_hart_to_entry_point(0)
        .map_err(|e| format!("failed to init hart: {:?}", e))?;

    log::info!("machine initialized, hart 0 pc = {:?}", 
        machine.get_hart(0).map(|h| h.pc));

    Ok(machine)
}
