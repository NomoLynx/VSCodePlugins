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
use crate::machine::hart::{HartState, PC_INCREMENT};
use crate::machine::register_ref::{RegisterRef, RegisterType};
use crate::machine::runtime_value::RuntimeValue;

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
const PERF_BASE: i64 = 3000;
const MACHINE_BASE: i64 = 4000;

/// Convert DAP thread_id → hart_id.  Thread IDs start at 1, hart IDs start at 0.
/// Returns u64 to match HartId type exactly, avoiding 32-bit truncation.
fn thread_id_to_hart_id(thread_id: i64) -> u64 {
    if thread_id > 0 {
        (thread_id - 1) as u64
    } else {
        0
    }
}

/// Encode a hart_id into a variables_reference for a given scope base.
/// Scope bases are spaced 1000 apart; the hart_id is added as an offset.
/// This lets Variables/SetVariable resolve the correct hart without relying
/// on the global `selected_hart_id`, preventing register cross-talk between
/// threads (P0-8/P0-9).
fn encode_vref(scope_base: i64, hart_id: u64) -> i64 {
    scope_base + (hart_id as i64)
}

/// Decode (scope_base, hart_id) from a variables_reference.
/// Returns None if the reference doesn't match any known scope.
fn decode_vref(vr: i64) -> Option<(i64, u64)> {
    let base = (vr / 1000) * 1000;
    let offset = vr - base;
    // Only decode positive offsets — negative would indicate a corrupted ref.
    let hart_id = if offset >= 0 { offset as u64 } else { return None; };
    match base {
        INT_REG_BASE | FP_REG_BASE | PERF_BASE | MACHINE_BASE => Some((base, hart_id)),
        _ => None,
    }
}

/// Helper: resolve hart_id from a variables_reference.  For scope bases that
/// are per-hart (INT_REG_BASE, FP_REG_BASE), the hart_id is encoded in the
/// reference.  For global scopes (PERF_BASE, MACHINE_BASE), falls back to
/// selected_hart_id.
fn hart_id_from_vref(vr: i64, selected_hart_id: u64) -> u64 {
    decode_vref(vr)
        .map(|(base, hid)| match base {
            INT_REG_BASE | FP_REG_BASE => hid,
            _ => selected_hart_id,
        })
        .unwrap_or(selected_hart_id)
}

/// Display a float register value with NaN-box detection.
/// RISC-V single-precision values are stored with upper 32 bits = 0xFFFFFFFF
/// (NaN-boxing).  If we blindly interpret such a pattern as f64, it shows NaN.
/// This helper detects the boxing and formats the value as f32 instead.
fn format_float_reg(bits: u64) -> String {
    if (bits >> 32) == 0xFFFFFFFF {
        format!("{} (f32)", f32::from_bits(bits as u32))
    } else {
        format!("{}", f64::from_bits(bits))
    }
}

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
    let mut offset_to_line_map: HashMap<usize, HashMap<usize, usize>> = HashMap::new();
    // Reverse map: program_id → source_line → Vec<offset>.
    // Built alongside offset_to_line_map so breakpoint logic can map a line to
    // all instructions at that line (needed for stepping past multi-instruction
    // source lines in Continue when hitting a breakpoint).
    let mut line_to_offsets: HashMap<usize, HashMap<usize, Vec<usize>>> = HashMap::new();
    // Source file path → program_id mapping, built during Launch so
    // SetBreakpoints/GotoTargets/BreakpointLocations can match by source.path.
    let mut source_to_program: HashMap<String, usize> = HashMap::new();
    // Breakpoints keyed by (program_id → line → bp_id).
    // Storing per-program avoids false hits when different programs share line numbers.
    let mut breakpoints_map: HashMap<usize, HashMap<i64, i64>> = HashMap::new();
    // P0-16: Globally unique breakpoint ID allocator shared by all programs.
    // DAP requires breakpoint IDs to be unique across the entire session so
    // hit_breakpoint_ids in StoppedEvent can be disambiguated without a source reference.
    let mut next_bp_id: i64 = 1;
    // Monotonically increasing frame ID — VS Code uses this to detect that
    // the stack has changed and will re-request Scopes/Variables.
    let mut frame_id_counter: i64 = 0;
    // Maps frame_id → hart_id so RestartFrame can resolve the correct hart
    // even when the user has been browsing frames from different threads.
    let mut frame_to_hart: HashMap<i64, u64> = HashMap::new();
    // Currently selected hart ID (set during StackTrace, used by Variables)
    let mut selected_hart_id: u64 = 0;

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

        // Clone command so the match doesn't borrow req, allowing
        // req.error() / req.success() to move req within any arm.
        let command = req.command.clone();
        match &command {
            Command::Initialize(_args) => {
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
                log::info!("breakpointLocations: line={}, source={:?}", args.line, args.source);

                let mut locations = Vec::new();
                let target_line = args.line as usize;

                // P1: Filter by source.path so multi-program debugging doesn't
                // falsely report breakpoint-able lines from the wrong program.
                let has_instruction = args
                    .source
                    .path
                    .as_ref()
                    .map(|p| canonicalize_path(p))
                    .and_then(|path| source_to_program.get(&path))
                    .and_then(|&pid| offset_to_line_map.get(&pid))
                    .map(|otl| otl.values().any(|&line| line == target_line))
                    .unwrap_or(false);
                
                // If offset_to_line_map is empty (program not loaded yet), 
                // still report the line as breakpoint-able so VSCode shows the breakpoint UI
                if has_instruction || offset_to_line_map.is_empty() {
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

                // P0-3: Reset frame→hart mappings on new session.
                frame_to_hart.clear();

                // load and assemble the .rv.s file
                if let Some(ref path) = program_path {
                    match load_and_assemble(path, &mut offset_to_line_map, &mut line_to_offsets, &mut source_to_program) {
                        Ok(m) => {
                            // Set initial line from hart 0's PC
                            if let Some(hart) = m.get_hart(0) {
                                if let Some(line) = offset_to_line_map
                                    .get(&hart.program_id)
                                    .and_then(|otl| otl.get(&hart.pc))
                                {
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
                let mut threads = Vec::new();
                if let Some(ref m) = machine {
                    for (pid, hart) in m.get_all_harts() {
                        // Thread ID = hart ID + 1 (DAP uses positive integers)
                        let tid = (hart.id + 1) as i64;
                        threads.push(types::Thread {
                            id: tid,
                            name: format!("hart{} (proc{})", hart.id, pid),
                        });
                    }
                }
                if threads.is_empty() {
                    threads.push(types::Thread {
                        id: 1,
                        name: "hart0".to_string(),
                    });
                }
                server.respond(req.success(
                    ResponseBody::Threads(ThreadsResponse {
                        threads,
                    })
                ))?;
            }
            Command::StackTrace(args) => {
                log::info!("stackTrace: thread_id={}, {:?}", args.thread_id, args);

                let hart_id = thread_id_to_hart_id(args.thread_id);
                selected_hart_id = hart_id;

                // Determine current_line from the selected hart's PC.
                // P0-2: Use nearest-line fallback so the UI never shows a stale line
                // after a jump to an address without direct source mapping.
                let hart_current_line = if let Some(ref m) = machine {
                    if let Some(hart) = m.get_hart(hart_id) {
                        offset_to_line_map
                            .get(&hart.program_id)
                            .and_then(|otl| otl.get(&hart.pc))
                            .copied()
                            .or_else(|| find_nearest_line(&offset_to_line_map, hart.program_id, hart.pc))
                            .unwrap_or(current_line)
                    } else {
                        current_line
                    }
                } else {
                    current_line
                };

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
                // Record frame → hart mapping so RestartFrame can resolve
                // the correct hart even when browsing frames from other threads.
                frame_to_hart.insert(frame_id_counter, hart_id);

                let sf = types::StackFrame {
                    id: frame_id_counter,
                    name: format!("hart{}_main", hart_id),
                    line: hart_current_line as i64,
                    column: 1,
                    end_line: Some(hart_current_line as i64),
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
                log::info!("STEP IN, thread_id={}", args.thread_id);
                // P0-3: Clear stale frame→hart mappings from previous stop cycle.
                // New StackTrace requests will populate fresh entries.
                frame_to_hart.clear();
                let hart_id = thread_id_to_hart_id(args.thread_id);
                selected_hart_id = hart_id;
                let outcome = do_single_step(&mut machine, hart_id, &mut current_line, &offset_to_line_map);

                if let StepOutcome::Terminated | StepOutcome::Error(_) = outcome {
                    // P0-4: Report execution errors so the user can distinguish
                    // normal termination from fatal errors (illegal instruction,
                    // memory access fault, etc.).
                    if let StepOutcome::Error(ref msg) = outcome {
                        server.send_event(Event::Output(OutputEventBody {
                            category: Some(types::OutputEventCategory::Stderr),
                            output: format!("[ERROR] {}\n", msg),
                            ..Default::default()
                        }))?;
                    }
                    server.respond(req.success(ResponseBody::StepIn))?;
                    // P0-4: Report non-zero exit code on execution error so
                    // automation/scripts can distinguish crash from normal exit.
                    let exit_code = if matches!(outcome, StepOutcome::Error(_)) { 1 } else { 0 };
                    server.send_event(Event::Exited(ExitedEventBody { exit_code }))?;
                    server.send_event(Event::Terminated(None))?;
                    break;
                }

                let hit_bp_id = machine
                    .as_ref()
                    .and_then(|m| m.get_hart(hart_id))
                    .and_then(|h| breakpoints_map.get(&h.program_id))
                    .and_then(|bp_table| bp_table.get(&(current_line as i64)))
                    .copied();
                let stop_reason = if hit_bp_id.is_some() { types::StoppedEventReason::Breakpoint } else { types::StoppedEventReason::Step };
                let hit_ids: Vec<i64> = if let Some(id) = hit_bp_id { vec![id] } else { vec![] };
                let desc = if hit_bp_id.is_some() { "breakpoint hit" } else { "step in" }; 

                // Send StoppedEvent BEFORE response so VSCode enters stopped state
                // and properly requests Scopes/Variables for updated register display.
                server.send_event(Event::Stopped(StoppedEventBody {
                    reason: stop_reason,
                    description: Some(desc.into()),
                    thread_id: Some((hart_id as i64) + 1),
                    preserve_focus_hint: Some(false),
                    text: None,
                    all_threads_stopped: Some(true),
                    hit_breakpoint_ids: Some(hit_ids),
                }))?;

                server.respond(req.success(ResponseBody::StepIn))?;
            }
            Command::Next(args) => {
                log::info!("STEP OVER, thread_id={}", args.thread_id);
                // P0-3: Clear stale frame→hart mappings from previous stop cycle.
                frame_to_hart.clear();
                let hart_id = thread_id_to_hart_id(args.thread_id);
                selected_hart_id = hart_id;
                let outcome = do_single_step(&mut machine, hart_id, &mut current_line, &offset_to_line_map);

                if let StepOutcome::Terminated | StepOutcome::Error(_) = outcome {
                    // P0-4: Report execution errors so the user can distinguish
                    // normal termination from fatal errors (illegal instruction,
                    // memory access fault, etc.).
                    if let StepOutcome::Error(ref msg) = outcome {
                        server.send_event(Event::Output(OutputEventBody {
                            category: Some(types::OutputEventCategory::Stderr),
                            output: format!("[ERROR] {}\n", msg),
                            ..Default::default()
                        }))?;
                    }
                    server.respond(req.success(ResponseBody::Next))?;
                    // P0-4: Report non-zero exit code on execution error so
                    // automation/scripts can distinguish crash from normal exit.
                    let exit_code = if matches!(outcome, StepOutcome::Error(_)) { 1 } else { 0 };
                    server.send_event(Event::Exited(ExitedEventBody { exit_code }))?;
                    server.send_event(Event::Terminated(None))?;
                    break;
                }

                let hit_bp_id = machine
                    .as_ref()
                    .and_then(|m| m.get_hart(hart_id))
                    .and_then(|h| breakpoints_map.get(&h.program_id))
                    .and_then(|bp_table| bp_table.get(&(current_line as i64)))
                    .copied();
                let stop_reason = if hit_bp_id.is_some() { types::StoppedEventReason::Breakpoint } else { types::StoppedEventReason::Step };
                let hit_ids: Vec<i64> = if let Some(id) = hit_bp_id { vec![id] } else { vec![] };
                let desc = if hit_bp_id.is_some() { "breakpoint hit" } else { "step next" }; 

                // Send StoppedEvent BEFORE response so VSCode enters stopped state
                // and properly requests Scopes/Variables for updated register display.
                server.send_event(Event::Stopped(StoppedEventBody {
                    reason: stop_reason,
                    description: Some(desc.into()),
                    thread_id: Some((hart_id as i64) + 1),
                    preserve_focus_hint: Some(false),
                    text: None,
                    all_threads_stopped: Some(true),
                    hit_breakpoint_ids: Some(hit_ids),
                }))?;

                server.respond(req.success(ResponseBody::Next))?;
            }
            Command::Continue(args) => {
                log::info!("CONTINUE, thread_id={}", args.thread_id);
                let hart_id = thread_id_to_hart_id(args.thread_id);
                selected_hart_id = hart_id;

                // P0-3: Clear stale frame→hart mappings from previous stop
                // cycle. New StackTrace calls will populate fresh entries.
                frame_to_hart.clear();

                let mut should_terminate = false;
                let mut had_execution_error = false;
                let mut hit_breakpoint = false;
                let mut hit_bp_ids: Vec<i64> = Vec::new();
                let mut steps = 0;
                let mut paused_by_user = false;
                // P0-1: Poll DAP every 10 instructions (was 1000) so runtime
                // SetBreakpoints take effect with ≤10-instruction latency.
                // A PeekNamedPipe on every 10th step is negligible overhead.
                const DAP_POLL_INTERVAL: usize = 10;

                if let Some(ref mut m) = machine {
                    let use_parallel = m.hart_count() > 1;
                    // P0-2: Before executing any instruction, check if any
                    // hart's current PC is already sitting on a breakpoint.
                    // Without this, when PC is manually placed at a breakpoint
                    // address (via Goto, RestartFrame, or manual register
                    // modification), Continue would execute the breakpoint-line
                    // instruction first and then stop at the NEXT instruction,
                    // silently skipping the breakpoint.
                    // In multi-hart mode, check ALL harts — not just the selected one.
                    {
                        let harts_to_check: Vec<u64> = if use_parallel {
                            m.get_all_harts().iter().map(|(_, h)| h.id).collect()
                        } else {
                            vec![hart_id]
                        };
                        for &hid in &harts_to_check {
                            // P0-22: Extract line from PC→line lookup so
                            // current_line stays in sync with the bp-hitting
                            // hart (previously the closure chain discarded it).
                            let bp_hit = m.get_hart(hid).and_then(|h| {
                                let pc_line = offset_to_line_map
                                    .get(&h.program_id)
                                    .and_then(|otl| otl.get(&h.pc))
                                    .copied();
                                pc_line.and_then(|line| {
                                    breakpoints_map
                                        .get(&h.program_id)
                                        .and_then(|bp| bp.get(&(line as i64)))
                                        .map(|&bp_id| (line, bp_id))
                                })
                            });
                            if let Some((line, bp_id)) = bp_hit {
                                if !hit_breakpoint {
                                    selected_hart_id = hid;
                                    current_line = line;
                                }
                                hit_breakpoint = true;
                                hit_bp_ids.push(bp_id);
                                log::info!(
                                    "continue: already at breakpoint line {} (hart {})",
                                    line, hid
                                );
                            }
                        }
                    }
                    // P0-24: When the pre-loop check finds PC already at a breakpoint,
                    // step past ALL instructions on that source line before entering
                    // the execution loop.  Without this, a source line with multiple
                    // instructions (pseudo-instruction expansion, macro expansion)
                    // would cause repeated breakpoint hits on the same line, which
                    // is confusing and breaks "stop before execution" semantics.
                    if hit_breakpoint {
                        let bp_hart_id = selected_hart_id;
                        let bp_program_id = m.get_hart(bp_hart_id)
                            .map(|h| h.program_id)
                            .unwrap_or(0);
                        let bp_line = current_line;
                        // Find the maximum offset for this source line (last
                        // instruction in the expansion).  Step past it so all
                        // instructions on the breakpoint line are executed.
                        let max_offset_for_line = line_to_offsets
                            .get(&bp_program_id)
                            .and_then(|l2o| l2o.get(&bp_line))
                            .and_then(|offsets| offsets.last().copied());
                        if let Some(max_off) = max_offset_for_line {
                            loop {
                                // P0-17: In multi-hart mode step_all_harts
                                // advances ALL harts together.  The original
                                // code only checked bp_hart_id's PC, causing
                                // other harts on the same breakpoint line to
                                // potentially keep unexecuted instructions
                                // when the loop breaks early.
                                let all_past = if use_parallel {
                                    m.get_all_harts().iter().all(|(_, h)| {
                                        h.program_id != bp_program_id
                                            || h.pc > max_off
                                            || offset_to_line_map
                                                .get(&h.program_id)
                                                .and_then(|otl| otl.get(&h.pc))
                                                != Some(&bp_line)
                                    })
                                } else {
                                    m.get_hart(bp_hart_id)
                                        .map(|h| h.pc > max_off)
                                        .unwrap_or(true)
                                };
                                if all_past {
                                    break;
                                }
                                let step_res = if use_parallel {
                                    m.step_all_harts().map(|_| ())
                                } else {
                                    m.step_hart(bp_hart_id)
                                };
                                if step_res.is_err() {
                                    break;
                                }
                                // Update current_line from the selected hart
                                if let Some(hart) = m.get_hart(bp_hart_id) {
                                    if let Some(line) = offset_to_line_map
                                        .get(&hart.program_id)
                                        .and_then(|otl| otl.get(&hart.pc))
                                    {
                                        current_line = *line;
                                    }
                                }
                            }
                        }
                        // Reset breakpoint flags — we've stepped past the line
                        hit_breakpoint = false;
                        hit_bp_ids.clear();

                        // P0-14: After stepping past the original breakpoint line,
                        // check whether any hart's PC now lands on the NEXT line's
                        // breakpoint BEFORE entering the main execution loop.
                        // The main loop is "step first, check later", so if line N+1
                        // also has a breakpoint, its first instruction would be
                        // executed before detection — violating "stop before
                        // execution" semantics for consecutive breakpoint lines.
                        let bp_check_harts: Vec<u64> = if use_parallel {
                            m.get_all_harts().iter().map(|(_, h)| h.id).collect()
                        } else {
                            vec![bp_hart_id]
                        };
                        for &hid in &bp_check_harts {
                            if let Some(hart) = m.get_hart(hid) {
                                if let Some(line) = offset_to_line_map
                                    .get(&hart.program_id)
                                    .and_then(|otl| otl.get(&hart.pc))
                                {
                                    if let Some(&bp_id) = breakpoints_map
                                        .get(&hart.program_id)
                                        .and_then(|bp| bp.get(&(*line as i64)))
                                    {
                                        if !hit_breakpoint {
                                            selected_hart_id = hid;
                                            current_line = *line;
                                        }
                                        hit_breakpoint = true;
                                        hit_bp_ids.push(bp_id);
                                        log::info!(
                                            "continue: PC landed on next breakpoint line {} (hart {}) after stepping past previous BP",
                                            current_line, hid
                                        );
                                    }
                                }
                            }
                        }
                    }
                    if hit_breakpoint {
                        // P0-24/P1-21: PC is already at a breakpoint line
                        // after pre-loop stepping.  Notify VSCode and respond
                        // to Continue so the UI shows the current state.
                        let desc = format!("breakpoint hit at line {}", current_line);
                        server.send_event(Event::Stopped(StoppedEventBody {
                            reason: types::StoppedEventReason::Breakpoint,
                            description: Some(desc),
                            thread_id: Some((selected_hart_id as i64) + 1),
                            preserve_focus_hint: Some(false),
                            text: None,
                            all_threads_stopped: Some(true),
                            hit_breakpoint_ids: Some(hit_bp_ids.clone()),
                        }))?;
                        server.respond(req.success(ResponseBody::Continue(
                            ContinueResponse { all_threads_continued: None }
                        )))?;
                    } else {
                        // P1-21: DAP expects an immediate Continue response
                        // (async model).  Respond now, then execute.  Post-loop
                        // events (StoppedEvent / Exited / Terminated) follow
                        // after the execution loop finishes.
                        server.respond(req.success(ResponseBody::Continue(
                            ContinueResponse { all_threads_continued: None }
                        )))?;

                        loop {
                            let step_result = if use_parallel {
                                m.step_all_harts().map(|_| ())
                            } else {
                                m.step_hart(hart_id)
                            };

                        match step_result {
                            Ok(()) => {
                                steps += 1;


                                // P0-2: Check for hart execution errors that are silently
                                // swallowed in multi-hart mode. Any hart that finished with
                                // an error must be reported to the user immediately.
                                // P0-26: Do NOT terminate the entire session on a single
                                // hart error.  Report the error as stderr output and
                                // continue executing remaining harts.  Only terminate when
                                // ALL harts have reached Finished (checked below).
                                let err_harts: Vec<String> = m
                                    .get_all_harts()
                                    .iter()
                                    .filter_map(|(_, h)| {
                                        if matches!(h.state, HartState::Finished) {
                                            h.error_info.as_ref().map(|e| {
                                                format!("hart{} execution error: {}", h.id, e)
                                            })
                                        } else {
                                            None
                                        }
                                    })
                                    .collect();
                                if !err_harts.is_empty() {
                                    let msg = err_harts.join("\n");
                                    log::error!("continue: hart error(s) detected: {}", msg);
                                    server.send_event(Event::Output(OutputEventBody {
                                        category: Some(types::OutputEventCategory::Stderr),
                                        output: format!("[ERROR] {}\n", msg),
                                        ..Default::default()
                                    }))?;
                                    had_execution_error = true;
                                    // P0-26: Don't set should_terminate here — let the
                                    // all_finished check below handle termination, so
                                    // remaining Running harts can continue executing.
                                }


                                // Check all harts for breakpoint hits (parallel mode)
                                // or just the selected one (single mode)
                                let harts_to_check: Vec<u64> = if use_parallel {
                                    m.get_all_harts().iter().map(|(_, h)| h.id).collect()
                                } else {
                                    vec![hart_id]
                                };

                                let mut this_round_hit_bp = false;
                                for &hid in &harts_to_check {
                                    if let Some(hart) = m.get_hart(hid) {
                                        if let Some(line) = offset_to_line_map
                                            .get(&hart.program_id)
                                            .and_then(|otl| otl.get(&hart.pc))
                                        {
                                            // Update current view if this is the selected hart
                                            if hid == selected_hart_id {
                                                current_line = *line;
                                            }
                                            // Check breakpoint hit (per-program)
                                            if let Some(&bp_id) = breakpoints_map
                                                .get(&hart.program_id)
                                                .and_then(|bp_table| bp_table.get(&(*line as i64)))
                                            {
                                                // First hit becomes the selected/focused hart
                                                if !this_round_hit_bp {
                                                    selected_hart_id = hid;
                                                    current_line = *line;
                                                }
                                                hit_breakpoint = true;
                                                hit_bp_ids.push(bp_id);
                                                log::info!("breakpoint hit at line {} (hart {}) after {} steps", current_line, hid, steps);
                                                if let Some(h) = m.get_hart(hid) {
                                                    log::info!("at breakpoint: t1(x6)={:x}, t2(x7)={:x}, t0(x5)={:x}, a0(x10)={:x}, a1(x11)={:x}, a2(x12)={:x}, a3(x13)={:x}",
                                                        h.x.read(6), h.x.read(7), h.x.read(5),
                                                        h.x.read(10), h.x.read(11), h.x.read(12),
                                                        h.x.read(13));
                                                }
                                                this_round_hit_bp = true;
                                                // Do NOT break — collect ALL harts' breakpoint hits
                                            }
                                        }
                                    }
                                }

                                if this_round_hit_bp {
                                    break;
                                }

                                // Check termination: all harts finished
                                // P1-24: step_hart_internal already marks a hart as
                                // Finished when PC goes past the end of inst_cache.
                                // This check covers both single-hart and multi-hart
                                // modes — the old has_inst_at_pc guard was redundant
                                // in single-hart mode (Finished ⟺ ¬has_inst_at_pc).
                                let all_finished = m.get_all_harts()
                                    .iter()
                                    .all(|(_, h)| matches!(h.state, HartState::Finished));
                                if all_finished {
                                    should_terminate = true;
                                    log::info!("all harts finished after {} steps", steps);
                                    break;
                                }

                                // Poll for pending DAP requests every N steps.
                                // Only Pause/Terminate/Disconnect interrupt execution;
                                // all other requests (Threads, StackTrace, Scopes, Variables,
                                // SetBreakpoints, etc.) are handled transparently and the
                                // simulation loop resumes automatically.
                                if steps % DAP_POLL_INTERVAL == 0 && stdin_has_data() {
                                    log::info!("continue: DAP data pending at step {}, processing", steps);
                                    match server.poll_request()? {
                                        Some(pending_req) => {
                                            match &pending_req.command {
                                                Command::Pause(_) => {
                                                    log::info!("continue: Pause received, yielding");
                                                    server.respond(pending_req.ack()?)?;
                                                    // Send StoppedEvent BEFORE responding to Continue
                                                    // so VSCode enters stopped state and refreshes UI.
                                                    server.send_event(Event::Stopped(StoppedEventBody {
                                                        reason: types::StoppedEventReason::Pause,
                                                        description: Some("paused by user".into()),
                                                        // P1: Use selected_hart_id so the focused
                                                        // thread is reported consistently with the
                                                        // standalone Pause handler.
                                                        thread_id: Some((selected_hart_id as i64) + 1),
                                                        preserve_focus_hint: Some(false),
                                                        text: None,
                                                        all_threads_stopped: Some(true),
                                                        hit_breakpoint_ids: Some(vec![]),
                                                    }))?;
                                                    paused_by_user = true;
                                                    break;
                                                }
                                                Command::Terminate(_) => {
                                                    log::info!("continue: Terminate received");
                                                    server.respond(pending_req.success(ResponseBody::Terminate))?;
                                                    should_terminate = true;
                                                    break;
                                                }
                                                Command::Disconnect(_) => {
                                                    log::info!("continue: Disconnect received");
                                                    server.respond(pending_req.success(ResponseBody::Disconnect))?;
                                                    should_terminate = true;
                                                    break;
                                                }
                                                Command::Threads => {
                                                    let mut threads = Vec::new();
                                                    for (pid, hart) in m.get_all_harts() {
                                                        let tid = (hart.id + 1) as i64;
                                                        threads.push(types::Thread {
                                                            id: tid,
                                                            name: format!("hart{} (proc{})", hart.id, pid),
                                                        });
                                                    }
                                                    if threads.is_empty() {
                                                        threads.push(types::Thread { id: 1, name: "hart0".to_string() });
                                                    }
                                                    server.respond(pending_req.success(
                                                        ResponseBody::Threads(ThreadsResponse { threads })
                                                    ))?;
                                                }
                                                Command::StackTrace(st_args) => {
                                                    // P0-3: Clear stale mappings so only this
                                                    // inline stop cycle's frames are tracked.
                                                    frame_to_hart.clear();
                                                    let st_hart_id = thread_id_to_hart_id(st_args.thread_id);
                                                    let st_line = if let Some(hart) = m.get_hart(st_hart_id) {
                                                        offset_to_line_map
                                                            .get(&hart.program_id)
                                                            .and_then(|otl| otl.get(&hart.pc))
                                                            .copied()
                                                            .or_else(|| find_nearest_line(&offset_to_line_map, hart.program_id, hart.pc))
                                                            .unwrap_or(current_line)
                                                    } else { current_line };
                                                    let st_file = program_path
                                                        .as_ref()
                                                        .and_then(|p| std::path::Path::new(p).file_name())
                                                        .and_then(|n| n.to_str())
                                                        .unwrap_or("program.rv.s")
                                                        .to_string();
                                                    frame_id_counter += 1;
                                                    // Record frame → hart mapping for RestartFrame.
                                                    frame_to_hart.insert(frame_id_counter, st_hart_id);
                                                    let sf = types::StackFrame {
                                                        id: frame_id_counter,
                                                        name: format!("hart{}_main", st_hart_id),
                                                        line: st_line as i64,
                                                        column: 1,
                                                        end_line: Some(st_line as i64),
                                                        end_column: Some(100),
                                                        source: Some(types::Source {
                                                            name: Some(st_file),
                                                            path: program_path.clone(),
                                                            presentation_hint: Some(types::PresentationHint::Normal),
                                                            ..Default::default()
                                                        }),
                                                        ..Default::default()
                                                    };
                                                    server.respond(pending_req.success(
                                                        ResponseBody::StackTrace(StackTraceResponse {
                                                            stack_frames: vec![sf],
                                                            total_frames: Some(1),
                                                        })
                                                    ))?;
                                                }
                                                Command::Scopes(s_args) => {
                                                    // P0-8/P0-9: Resolve the hart_id from frame_id so that
                                                    // Variables/SetVariable read the correct hart's registers,
                                                    // not the globally-last-selected hart.
                                                    let scope_hart_id = frame_to_hart
                                                        .get(&s_args.frame_id)
                                                        .copied()
                                                        .unwrap_or(selected_hart_id);
                                                    server.respond(pending_req.success(
                                                        ResponseBody::Scopes(ScopesResponse {
                                                            scopes: vec![
                                                                types::Scope {
                                                                    name: "Registers".to_string(),
                                                                    presentation_hint: Some(types::ScopePresentationhint::Registers),
                                                                    variables_reference: encode_vref(INT_REG_BASE, scope_hart_id),
                                                                    named_variables: Some(32),
                                                                    indexed_variables: None,
                                                                    expensive: false,
                                                                    source: None, line: None, column: None,
                                                                    end_line: None, end_column: None,
                                                                },
                                                                types::Scope {
                                                                    name: "Float Registers".to_string(),
                                                                    presentation_hint: Some(types::ScopePresentationhint::Registers),
                                                                    variables_reference: encode_vref(FP_REG_BASE, scope_hart_id),
                                                                    named_variables: Some(32),
                                                                    indexed_variables: None,
                                                                    expensive: false,
                                                                    source: None, line: None, column: None,
                                                                    end_line: None, end_column: None,
                                                                },
                                                                types::Scope {
                                                                    name: "Performance".to_string(),
                                                                    presentation_hint: None,
                                                                    // P1-21: PERF is a global view, no per-hart encoding needed
                                                                    variables_reference: PERF_BASE,
                                                                    named_variables: None,
                                                                    indexed_variables: None,
                                                                    expensive: false,
                                                                    source: None, line: None, column: None,
                                                                    end_line: None, end_column: None,
                                                                },
                                                                types::Scope {
                                                                    name: "Machine".to_string(),
                                                                    presentation_hint: None,
                                                                    // P1-21: MACHINE is a global view, no per-hart encoding needed
                                                                    variables_reference: MACHINE_BASE,
                                                                    named_variables: None,
                                                                    indexed_variables: None,
                                                                    expensive: false,
                                                                    source: None, line: None, column: None,
                                                                    end_line: None, end_column: None,
                                                                },
                                                            ],
                                                        })
                                                    ))?;
                                                }
                                                Command::Variables(v_args) => {
                                                    let mut vars: Vec<types::Variable> = Vec::new();
                                                    // P0-8/P0-9: Decode the hart_id from the
                                                    // variables_reference (encoded by Scopes handler),
                                                    // falling back to selected_hart_id for old clients.
                                                    let var_hart_id = hart_id_from_vref(
                                                        v_args.variables_reference,
                                                        selected_hart_id,
                                                    );
                                                    let vr_base = (v_args.variables_reference / 1000) * 1000;
                                                    if vr_base == INT_REG_BASE {
                                                        if let Some(hart) = m.get_hart(var_hart_id) {
                                                            for i in 0..32 {
                                                                let abi_name = INT_REG_NAMES[i];
                                                                vars.push(types::Variable {
                                                                    name: format!("x{}/{}", i, abi_name),
                                                                    value: format!("0x{:016x}", hart.x.read(i)),
                                                                    type_field: Some("u64".to_string()),
                                                                    evaluate_name: Some(format!("x{}", i)),
                                                                    variables_reference: 0,
                                                                    named_variables: None, indexed_variables: None,
                                                                    presentation_hint: None, memory_reference: None,
                                                                });
                                                            }
                                                        }
                                                    } else if vr_base == FP_REG_BASE {
                                                        if let Some(hart) = m.get_hart(var_hart_id) {
                                                            for i in 0..32 {
                                                                vars.push(types::Variable {
                                                                    name: format!("f{}", i),
                                                                    // P0-7/P0-11: Use NaN-box-aware formatter
                                                                    value: format_float_reg(hart.f.read(i)),
                                                                    type_field: Some("f64".to_string()),
                                                                    evaluate_name: Some(format!("f{}", i)),
                                                                    variables_reference: 0,
                                                                    named_variables: None, indexed_variables: None,
                                                                    presentation_hint: None, memory_reference: None,
                                                                });
                                                            }
                                                        }
                                                    } else if vr_base == PERF_BASE {
                                                        for perf in m.get_all_performances() {
                                                            let label = format!("hart{} (proc{})", perf.hart_id, perf.processor_id);
                                                            for (name, value, type_name) in &[
                                                                ("State", format!("{:?}", perf.state), "HartState"),
                                                                ("Runtime Cycles", format!("{}", perf.runtime_cycles), "u64"),
                                                                ("Elapsed Cycles", format!("{}", perf.elapsed_cycles), "u64"),
                                                                ("Exec Cycles", format!("{}", perf.exec_cycles), "u64"),
                                                                ("Inst Count", format!("{}", perf.inst_count), "u64"),
                                                                ("IPC", format!("{:.3}", perf.ipc()), "f64"),
                                                            ] {
                                                                vars.push(types::Variable {
                                                                    name: format!("{} {}", label, name),
                                                                    value: value.clone(),
                                                                    type_field: Some(type_name.to_string()),
                                                                    evaluate_name: None,
                                                                    variables_reference: 0,
                                                                    named_variables: None, indexed_variables: None,
                                                                    presentation_hint: None, memory_reference: None,
                                                                });
                                                            }
                                                        }
                                                    } else if vr_base == MACHINE_BASE {
                                                        let total = m.get_total_cycles();
                                                        let count = m.hart_count() as u64;
                                                        for pi in 0..m.processor_count() {
                                                            if let Some(clock) = m.get_processor_clock(pi) {
                                                                vars.push(types::Variable {
                                                                    name: format!("Processor {} Clock", pi),
                                                                    value: format!("{}", clock),
                                                                    type_field: Some("u64".to_string()),
                                                                    evaluate_name: None,
                                                                    variables_reference: 0,
                                                                    named_variables: None, indexed_variables: None,
                                                                    presentation_hint: None, memory_reference: None,
                                                                });
                                                            }
                                                        }
                                                        vars.push(types::Variable { name: "Global Clock (proc0)".into(), value: format!("{}", m.get_global_clock()), type_field: Some("u64".into()), evaluate_name: None, variables_reference: 0, named_variables: None, indexed_variables: None, presentation_hint: None, memory_reference: None });
                                                        vars.push(types::Variable { name: "Total Exec Cycles".into(), value: format!("{}", total), type_field: Some("u64".into()), evaluate_name: None, variables_reference: 0, named_variables: None, indexed_variables: None, presentation_hint: None, memory_reference: None });
                                                        vars.push(types::Variable { name: "Hart Count".into(), value: format!("{}", count), type_field: Some("u64".into()), evaluate_name: None, variables_reference: 0, named_variables: None, indexed_variables: None, presentation_hint: None, memory_reference: None });
                                                        vars.push(types::Variable { name: "Avg Cycles/Hart".into(), value: format!("{}", m.get_avg_cycles_per_hart()), type_field: Some("u64".into()), evaluate_name: None, variables_reference: 0, named_variables: None, indexed_variables: None, presentation_hint: None, memory_reference: None });
                                                    }
                                                    server.respond(pending_req.success(
                                                        ResponseBody::Variables(VariablesResponse { variables: vars })
                                                    ))?;
                                                }
                                                Command::SetBreakpoints(bp_args) => {
                                                    // P0-25 + P1-30: Match program by source.path only.
                                                    // No silent fallback — source.path must be in
                                                    // source_to_program, otherwise return verified: false.
                                                    let bp_program_id = bp_args.source.path.as_ref()
                                                        .map(|p| canonicalize_path(p))
                                                        .and_then(|path| source_to_program.get(&path))
                                                        .copied();

                                                    if bp_program_id.is_none() {
                                                        let msg = bp_args.source.path.as_ref()
                                                            .map(|p| format!("source file not found in any loaded program: {}", p))
                                                            .unwrap_or_else(|| "source file path not provided".into());
                                                        log::warn!("continue inline setBreakpoints: {}", msg);
                                                        let bp_count = bp_args.breakpoints.as_ref().map(|b| b.len()).unwrap_or(0);
                                                        let unverified_bps: Vec<types::Breakpoint> = (0..bp_count)
                                                            .map(|_| types::Breakpoint {
                                                                id: Some(next_bp_id),
                                                                verified: false,
                                                                message: Some(msg.clone()),
                                                                source: Some(bp_args.source.clone()),
                                                                line: None, column: None,
                                                                end_line: None, end_column: None,
                                                                instruction_reference: None, offset: None,
                                                            })
                                                            .collect();
                                                        next_bp_id += bp_count as i64;
                                                        server.respond(pending_req.success(
                                                            ResponseBody::SetBreakpoints(SetBreakpointsResponse { breakpoints: unverified_bps })
                                                        ))?;
                                                        // Skip hit check — unverified bps have no line
                                                    } else {
                                                        let bp_program_id = bp_program_id.unwrap();
                                                        let bp_table = breakpoints_map.entry(bp_program_id).or_default();
                                                        bp_table.clear();
                                                        let mut bp_list = Vec::new();
                                                        if let Some(ref source_bps) = bp_args.breakpoints {
                                                            for bp in source_bps {
                                                                // P0-16: Use global unique bp_id
                                                                let bp_id = next_bp_id;
                                                                next_bp_id += 1;
                                                                bp_table.insert(bp.line, bp_id);
                                                                bp_list.push(types::Breakpoint {
                                                                    id: Some(bp_id), verified: true, message: None,
                                                                    source: Some(bp_args.source.clone()),
                                                                    line: Some(bp.line), column: bp.column,
                                                                    end_line: None, end_column: None,
                                                                    instruction_reference: None, offset: None,
                                                                });
                                                            }
                                                        }
                                                        server.respond(pending_req.success(
                                                            ResponseBody::SetBreakpoints(SetBreakpointsResponse { breakpoints: bp_list })
                                                        ))?;
                                                    }

                                                    // P0-8: After runtime breakpoint updates, immediately
                                                    // check if any hart's current PC now sits on a new
                                                    // breakpoint. Without this check, the next loop iteration
                                                    // executes one instruction before detecting the hit,
                                                    // violating "stop before execution" semantics.
                                                    // P0-11: After detecting hits, break the outer execution
                                                    // loop so the instruction at the breakpoint is NOT
                                                    // executed (stop-before-execution semantics).
                                                    // P0-12: Collect ALL harts' breakpoint hits — do NOT
                                                    // break from the for loop early.
                                                    if !hit_breakpoint {
                                                        let check_harts: Vec<u64> = if use_parallel {
                                                            m.get_all_harts().iter().map(|(_, h)| h.id).collect()
                                                        } else {
                                                            vec![hart_id]
                                                        };
                                                        let mut setpoint_hit = false;
                                                        for &hid in &check_harts {
                                                            if let Some(hart) = m.get_hart(hid) {
                                                                if let Some(line) = offset_to_line_map
                                                                    .get(&hart.program_id)
                                                                    .and_then(|otl| otl.get(&hart.pc))
                                                                {
                                                                    if let Some(&bp_id) = breakpoints_map
                                                                        .get(&hart.program_id)
                                                                        .and_then(|bp| bp.get(&(*line as i64)))
                                                                    {
                                                                        if !hit_breakpoint {
                                                                            selected_hart_id = hid;
                                                                            current_line = *line;
                                                                        }
                                                                        hit_breakpoint = true;
                                                                        hit_bp_ids.push(bp_id);
                                                                        setpoint_hit = true;
                                                                        log::info!(
                                                                            "continue: new breakpoint hit at line {} (hart {}) after SetBreakpoints",
                                                                            current_line, hid
                                                                        );
                                                                        // Do NOT break — collect ALL harts' hits (P0-12)
                                                                    }
                                                                }
                                                            }
                                                        }
                                                        // P0-11: break the outer execution loop so
                                                        // the breakpoint-line instruction is NOT executed
                                                        if setpoint_hit {
                                                            break;
                                                        }
                                                    }
                                                }
                                                other => {
                                                    log::debug!("continue: ignoring request {:?} during execution", other);
                                                    server.respond(pending_req.ack()?)?;
                                                }
                                            }
                                        }
                                        None => {
                                            // Stream closed — terminate
                                            should_terminate = true;
                                            break;
                                        }
                                    }
                                }
                                // safety: max 100000 steps to avoid infinite loops
                                if steps > 100000 {
                                    log::warn!("continue reached max steps limit");
                                    break;
                                }
                            }
                            Err(e) => {
                                log::info!("continue error at step {}: {:?}", steps, e);
                                // step_all_harts only returns Err when all harts failed,
                                // and each failing hart is marked Finished by step_hart_internal,
                                // so no hart can be Running at this point.
                                // P0-17: Report execution error so automation/scripts can
                                // distinguish crash (exit_code=1) from normal termination (0).
                                had_execution_error = true;
                                // Collect error_info from all failed harts and
                                // send them to the debug console.
                                let err_harts: Vec<String> = m
                                    .get_all_harts()
                                    .iter()
                                    .filter_map(|(_, h)| {
                                        if matches!(h.state, HartState::Finished) {
                                            h.error_info.as_ref().map(|e| {
                                                format!("hart{} execution error: {}", h.id, e)
                                            })
                                        } else {
                                            None
                                        }
                                    })
                                    .collect();
                                if !err_harts.is_empty() {
                                    let msg = err_harts.join("\n");
                                    server.send_event(Event::Output(OutputEventBody {
                                        category: Some(types::OutputEventCategory::Stderr),
                                        output: format!("[ERROR] {}\n", msg),
                                        ..Default::default()
                                    }))?;
                                }
                                should_terminate = true;
                                break;
                            }
                        }
                    }

                        // Post-loop event handling (DAP async: Continue was
                        // already responded to above — we only send events here).
                        if should_terminate {
                            // P0-4: Non-zero exit code on execution error so
                            // automation/scripts can distinguish crash from normal exit.
                            let exit_code = if had_execution_error { 1 } else { 0 };
                            server.send_event(Event::Exited(ExitedEventBody { exit_code }))?;
                            server.send_event(Event::Terminated(None))?;
                            break;
                        } else if !paused_by_user {
                            // Breakpoint hit or step limit reached
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
                            let hit_ids: Vec<i64> = if hit_breakpoint { hit_bp_ids } else { vec![] };
                            server.send_event(Event::Stopped(StoppedEventBody {
                                reason: stop_reason,
                                description: Some(desc),
                                // P0-1: Use selected_hart_id (the hart that actually hit)
                                // instead of the original hart_id from the Continue request.
                                // In multi-hart mode a different hart may hit the breakpoint.
                                thread_id: Some((selected_hart_id as i64) + 1),
                                preserve_focus_hint: Some(false),
                                text: None,
                                all_threads_stopped: Some(true),
                                hit_breakpoint_ids: Some(hit_ids),
                            }))?;
                        }
                        // If paused_by_user: StoppedEvent already sent inline — skip.
                    } // else (DAP async execution path)
                } else {
                    // No machine loaded — just acknowledge
                    server.respond(req.success(ResponseBody::Continue(
                        ContinueResponse { all_threads_continued: None }
                    )))?;
                }

            }
            Command::Scopes(args) => {
                log::info!("scopes request: frame_id={}", args.frame_id);
                // P0-8/P0-9: Resolve hart_id from frame_id so Variables/SetVariable
                // read the correct hart's registers instead of the last-selected.
                let scope_hart_id = frame_to_hart
                    .get(&args.frame_id)
                    .copied()
                    .unwrap_or(selected_hart_id);

                server.respond(req.success(
                    ResponseBody::Scopes(ScopesResponse {
                        scopes: vec![
                            types::Scope {
                                name: "Registers".to_string(),
                                presentation_hint: Some(types::ScopePresentationhint::Registers),
                                variables_reference: encode_vref(INT_REG_BASE, scope_hart_id),
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
                                variables_reference: encode_vref(FP_REG_BASE, scope_hart_id),
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
                                name: "Performance".to_string(),
                                presentation_hint: None,
                                // P1-21: PERF is a global view, no per-hart encoding needed
                                variables_reference: PERF_BASE,
                                named_variables: None,
                                indexed_variables: None,
                                expensive: false,
                                source: None,
                                line: None,
                                column: None,
                                end_line: None,
                                end_column: None,
                            },
                            types::Scope {
                                name: "Machine".to_string(),
                                presentation_hint: None,
                                // P1-21: MACHINE is a global view, no per-hart encoding needed
                                variables_reference: MACHINE_BASE,
                                named_variables: None, // dynamic: per-processor + global
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
                // P0-8/P0-9: Decode hart_id from variables_reference instead of
                // using the global selected_hart_id, preventing register cross-talk.
                let var_hart_id = hart_id_from_vref(
                    args.variables_reference,
                    selected_hart_id,
                );
                let vr_base = (args.variables_reference / 1000) * 1000;

                if vr_base == INT_REG_BASE {
                    // Return integer registers for the selected hart
                    if let Some(ref m) = machine {
                        if let Some(hart) = m.get_hart(var_hart_id) {
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
                } else if vr_base == FP_REG_BASE {
                    // Return float registers for the selected hart
                    if let Some(ref m) = machine {
                        if let Some(hart) = m.get_hart(var_hart_id) {
                            for i in 0..32 {
                                let name = format!("f{}", i);
                                let value = hart.f.read(i);
                                variables.push(types::Variable {
                                    name,
                                    // P0-7/P0-11: Use NaN-box-aware formatter
                                    value: format_float_reg(value),
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
                } else if vr_base == PERF_BASE {
                    // Return per-hart performance counters for ALL harts
                    if let Some(ref m) = machine {
                        for perf in m.get_all_performances() {
                            let label = format!("hart{} (proc{})", perf.hart_id, perf.processor_id);
                            variables.push(types::Variable {
                                name: format!("{} State", label),
                                value: format!("{:?}", perf.state),
                                type_field: Some("HartState".to_string()),
                                evaluate_name: None,
                                variables_reference: 0,
                                named_variables: None,
                                indexed_variables: None,
                                presentation_hint: None,
                                memory_reference: None,
                            });
                            variables.push(types::Variable {
                                name: format!("{} Runtime Cycles", label),
                                value: format!("{}", perf.runtime_cycles),
                                type_field: Some("u64".to_string()),
                                evaluate_name: None,
                                variables_reference: 0,
                                named_variables: None,
                                indexed_variables: None,
                                presentation_hint: None,
                                memory_reference: None,
                            });
                            variables.push(types::Variable {
                                name: format!("{} Elapsed Cycles", label),
                                value: format!("{}", perf.elapsed_cycles),
                                type_field: Some("u64".to_string()),
                                evaluate_name: None,
                                variables_reference: 0,
                                named_variables: None,
                                indexed_variables: None,
                                presentation_hint: None,
                                memory_reference: None,
                            });
                            variables.push(types::Variable {
                                name: format!("{} Exec Cycles", label),
                                value: format!("{}", perf.exec_cycles),
                                type_field: Some("u64".to_string()),
                                evaluate_name: None,
                                variables_reference: 0,
                                named_variables: None,
                                indexed_variables: None,
                                presentation_hint: None,
                                memory_reference: None,
                            });
                            variables.push(types::Variable {
                                name: format!("{} Inst Count", label),
                                value: format!("{}", perf.inst_count),
                                type_field: Some("u64".to_string()),
                                evaluate_name: None,
                                variables_reference: 0,
                                named_variables: None,
                                indexed_variables: None,
                                presentation_hint: None,
                                memory_reference: None,
                            });
                            variables.push(types::Variable {
                                name: format!("{} IPC", label),
                                value: format!("{:.3}", perf.ipc()),
                                type_field: Some("f64".to_string()),
                                evaluate_name: None,
                                variables_reference: 0,
                                named_variables: None,
                                indexed_variables: None,
                                presentation_hint: None,
                                memory_reference: None,
                            });
                        }
                    }
                } else if vr_base == MACHINE_BASE {
                    // Return machine-wide stats — all data comes from simulator APIs
                    if let Some(ref m) = machine {
                        let total = m.get_total_cycles();
                        let count = m.hart_count() as u64;
                        // Show per-processor clocks to avoid misleading "global" label
                        for pi in 0..m.processor_count() {
                            if let Some(clock) = m.get_processor_clock(pi) {
                                variables.push(types::Variable {
                                    name: format!("Processor {} Clock", pi),
                                    value: format!("{}", clock),
                                    type_field: Some("u64".to_string()),
                                    evaluate_name: None,
                                    variables_reference: 0,
                                    named_variables: None,
                                    indexed_variables: None,
                                    presentation_hint: None,
                                    memory_reference: None,
                                });
                            }
                        }
                        // Backward-compatible "Global Clock" (processor 0 clock)
                        variables.push(types::Variable {
                            name: "Global Clock (proc0)".to_string(),
                            value: format!("{}", m.get_global_clock()),
                            type_field: Some("u64".to_string()),
                            evaluate_name: None,
                            variables_reference: 0,
                            named_variables: None,
                            indexed_variables: None,
                            presentation_hint: None,
                            memory_reference: None,
                        });
                        variables.push(types::Variable {
                            name: "Total Exec Cycles".to_string(),
                            value: format!("{}", total),
                            type_field: Some("u64".to_string()),
                            evaluate_name: None,
                            variables_reference: 0,
                            named_variables: None,
                            indexed_variables: None,
                            presentation_hint: None,
                            memory_reference: None,
                        });
                        variables.push(types::Variable {
                            name: "Hart Count".to_string(),
                            value: format!("{}", count),
                            type_field: Some("u64".to_string()),
                            evaluate_name: None,
                            variables_reference: 0,
                            named_variables: None,
                            indexed_variables: None,
                            presentation_hint: None,
                            memory_reference: None,
                        });
                        variables.push(types::Variable {
                            name: "Avg Cycles/Hart".to_string(),
                            value: format!("{}", m.get_avg_cycles_per_hart()),
                            type_field: Some("u64".to_string()),
                            evaluate_name: None,
                            variables_reference: 0,
                            named_variables: None,
                            indexed_variables: None,
                            presentation_hint: None,
                            memory_reference: None,
                        });
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

                // P0-8/P0-9: Decode hart_id from variables_reference instead of
                // using the global selected_hart_id.  Without this, SetVariable
                // modifies the wrong hart when the user switches stack frames.
                let setvar_hart_id = hart_id_from_vref(
                    args.variables_reference,
                    selected_hart_id,
                );
                let vr_base = (args.variables_reference / 1000) * 1000;

                if vr_base == INT_REG_BASE {
                    // Parse register index from name like "x5/t0"
                    if let Some(reg_name) = args.name.split('/').next() {
                        if reg_name.starts_with('x') {
                            if let Ok(idx) = reg_name[1..].parse::<usize>() {
                                    if idx < 32 {
                                    if let Ok(val) = parse_u64(&args.value) {
                                        // P0-3: x0 is hardwired to 0 per RISC-V ISA.
                                        // Writing to it is silently ignored by IntegerRegisterFile,
                                        // but the DAP layer previously reported success with the
                                        // user-supplied value — making the UI show false data.
                                        // Explicitly reject the write so the user gets honest feedback.
                                        if idx == 0 {
                                            server.respond(req.error(
                                                "x0 is hardwired to 0 per RISC-V ISA and cannot be written"
                                            ))?;
                                            continue;
                                        } else {
                                            if let Some(ref mut m) = machine {
                                                if let Some(hart) = m.get_hart_mut(setvar_hart_id) {
                                                    // Route through write_register for
                                                    // consistency with other write paths.
                                                    // P1-20: Propagate write errors instead
                                                    // of silently swallowing them with let _.
                                                    match hart.write_register(
                                                        &RegisterRef { reg_type: RegisterType::Integer, index: idx },
                                                        RuntimeValue::Integer(val),
                                                    ) {
                                                        Ok(()) => {
                                                            new_value = format!("0x{:016x}", val);
                                                            write_ok = true;
                                                        }
                                                        Err(e) => {
                                                            server.respond(req.error(
                                                                &format!("Failed to write x{}: {}", idx, e)
                                                            ))?;
                                                            continue;
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if !write_ok {
                        server.respond(req.error("Failed to parse integer register name or value"))?;
                        continue;
                    }
                } else if vr_base == FP_REG_BASE {
                    if let Some(reg_name) = args.name.split('/').next() {
                        if reg_name.starts_with('f') {
                            if let Ok(idx) = reg_name[1..].parse::<usize>() {
                                if idx < 32 {
                                    if let Ok(val) = args.value.parse::<f64>() {
                                        if let Some(ref mut m) = machine {
                                            if let Some(hart) = m.get_hart_mut(setvar_hart_id) {
                                                // Route through write_register for
                                                // consistency — avoids the dual-path
                                                // issue where DAP bypasses write_register.
                                                // P1-20: Propagate write errors instead
                                                // of silently swallowing them with let _.
                                                match hart.write_register(
                                                    &RegisterRef { reg_type: RegisterType::Float, index: idx },
                                                    RuntimeValue::Float64(val),
                                                ) {
                                                    Ok(()) => {
                                                        new_value = format!("{}", val);
                                                        write_ok = true;
                                                    }
                                                    Err(e) => {
                                                        server.respond(req.error(
                                                            &format!("Failed to write f{}: {}", idx, e)
                                                        ))?;
                                                        continue;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if !write_ok {
                        server.respond(req.error("Failed to parse float register name or value"))?;
                        continue;
                    }
                } else {
                    // PERF_BASE, MACHINE_BASE, or any other scope —
                    // SetVariable is only supported for register scopes.
                    let err_ref = args.variables_reference;
                    server.respond(req.error(&format!(
                        "SetVariable is only supported for Registers and Float Registers. \
                         Scope ref={} does not support modification.",
                        err_ref
                    )))?;
                    continue;
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

                // Return all instructions in the source file's program as goto targets.
                // Each target ID encodes both the program_id and instruction offset
                // so Goto can validate cross-program jumps (P0-4):
                //   target_id = (program_id << 32) | offset
                // This prevents a Goto from using a target generated for a different
                // program where the same offset happens to be valid.
                let mut targets = Vec::new();
                let program_id = args
                    .source
                    .path
                    .as_ref()
                    .map(|p| canonicalize_path(p))
                    .and_then(|path| source_to_program.get(&path))
                    .copied();
                if let (Some(ref m), Some(pid)) = (machine.as_ref(), program_id) {
                    if pid < m.programs.len() {
                            let prog = &m.programs[pid];
                            let items = prog.get_text_section_items();
                            for (idx, item) in items.iter().enumerate() {
                                // Only include items that have an instruction and a source line
                                if item.get_inc().is_none() {
                                    continue;
                                }
                                let offset = item.get_offset();
                                let line = offset_to_line_map
                                    .get(&pid)
                                    .and_then(|otl| otl.get(&offset))
                                    .copied()
                                    .unwrap_or(idx + 1) as i64;
                                targets.push(types::GotoTarget {
                                    // P0-4: Encode program_id in target_id so Goto can
                                    // validate that the target belongs to the same program
                                    // as the hart being jumped.
                                    id: ((pid as i64) << 32) | (offset as i64),
                                    label: format!("Line {} (offset {:#x})", line, offset),
                                    line,
                                    column: Some(1),
                                    end_line: None,
                                    end_column: None,
                                    instruction_pointer_reference: Some(format!("{}", idx)),
                                });
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
                let target_id = args.target_id;
                let thread_id = args.thread_id;
                log::info!("goto: thread_id={}, target_id={}", thread_id, target_id);

                let hart_id = thread_id_to_hart_id(thread_id);
                selected_hart_id = hart_id;

                // target_id encodes both program_id (upper 32 bits) and offset
                // (lower 32 bits), as set in GotoTargets.  Decode both to verify
                // the target belongs to the current hart's program.
                let target_program_id = (target_id >> 32) as usize;
                let target_offset = (target_id & 0xFFFF_FFFF) as usize;

                let hart_program_id = machine.as_ref()
                    .and_then(|m| m.get_hart(hart_id))
                    .map(|h| h.program_id)
                    .unwrap_or(0);

                if target_program_id != hart_program_id {
                    server.respond(req.error(&format!(
                        "Goto target_id {} belongs to program {}, but hart {} runs program {}",
                        target_id, target_program_id, hart_id, hart_program_id
                    )))?;
                    continue;
                }

                // Verify the offset actually maps to a known instruction
                let inst_exists = machine.as_ref()
                    .and_then(|m| m.programs.get(target_program_id))
                    .map(|prog| {
                        prog.get_text_section_items()
                            .iter()
                            .any(|item| item.get_offset() == target_offset && item.get_inc().is_some())
                    })
                    .unwrap_or(false);

                if inst_exists {
                    if let Some(ref mut m) = machine {
                        if let Some(hart) = m.get_hart_mut(hart_id) {
                            hart.set_pc(target_offset);
                            // Reset state so a Finished hart can resume execution
                            if matches!(hart.state, HartState::Finished) {
                                hart.state = HartState::Running;
                                hart.finish_clock = None;
                                hart.error_info = None;
                            }
                            // Look up source line for the display
                            if let Some(line) = offset_to_line_map
                                .get(&target_program_id)
                                .and_then(|otl| otl.get(&target_offset))
                            {
                                current_line = *line;
                            }
                            log::info!("goto: set pc to {:#x} (line {})", target_offset, current_line);
                        }
                    }

                    server.respond(req.success(ResponseBody::Goto))?;

                    server.send_event(Event::Stopped(StoppedEventBody {
                        reason: types::StoppedEventReason::Goto,
                        description: Some(format!("jumped to offset {:#x} (line {})", target_offset, current_line)),
                        thread_id: Some((hart_id as i64) + 1),
                        preserve_focus_hint: Some(false),
                        text: None,
                        all_threads_stopped: Some(true),
                        hit_breakpoint_ids: Some(vec![]),
                    }))?;
                } else {
                    server.respond(req.error(&format!("No code mapped to target_id {} (offset {:#x})", target_id, target_offset)))?;
                }
            }
            Command::SetBreakpoints(args) => {
                log::info!("setBreakpoints: source.path={:?}, source.name={:?}", 
                    args.source.path, args.source.name);
                log::info!("setBreakpoints: breakpoints={:?}", args.breakpoints);
                log::info!("setBreakpoints: lines={:?}", args.lines);

                // Determine which program these breakpoints belong to.
                // Must match source.path exactly against the source_to_program
                // map built during Launch.  Any fallback (selected_hart_id or
                // default 0) would silently route breakpoints to the wrong
                // program — instead, return verified: false so VSCode shows
                // the hollow-grey-circle indicator and logs the error reason.
                let program_id = args
                    .source
                    .path
                    .as_ref()
                    .map(|p| canonicalize_path(p))
                    .and_then(|path| source_to_program.get(&path))
                    .copied();

                if program_id.is_none() {
                    // Source file not registered — not in any loaded program.
                    // Return all breakpoints as unverified so the user can see
                    // they didn't take effect and knows why.
                    let msg = args
                        .source
                        .path
                        .as_ref()
                        .map(|p| format!("source file not found in any loaded program: {}", p))
                        .unwrap_or_else(|| "source file path not provided".into());
                    log::warn!("setBreakpoints: {}", msg);

                    let bp_count = args.breakpoints.as_ref()
                        .map(|b| b.len())
                        .or_else(|| args.lines.as_ref().map(|l| l.len()))
                        .unwrap_or(0);
                    let unverified_bps: Vec<types::Breakpoint> = (0..bp_count)
                        .map(|_| types::Breakpoint {
                            id: Some(next_bp_id),
                            verified: false,
                            message: Some(msg.clone()),
                            source: Some(args.source.clone()),
                            line: None,
                            column: None,
                            end_line: None,
                            end_column: None,
                            instruction_reference: None,
                            offset: None,
                        })
                        .collect();
                    // Advance bp_id to keep ids unique even for unverified
                    next_bp_id += bp_count as i64;

                    server.respond(req.success(
                        ResponseBody::SetBreakpoints(SetBreakpointsResponse {
                            breakpoints: unverified_bps,
                        })
                    ))?;
                } else {
                    let program_id = program_id.unwrap();

                    // Clear old breakpoints for this program and set new ones
                    let bp_table = breakpoints_map.entry(program_id).or_default();
                    bp_table.clear();
                    let mut bp_list = Vec::new();
                    if let Some(ref source_bps) = args.breakpoints {
                        for bp in source_bps {
                            // P0-16: Use global unique bp_id
                            let bp_id = next_bp_id;
                            next_bp_id += 1;
                            bp_table.insert(bp.line, bp_id);
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
                            // P0-16: Use global unique bp_id
                            let bp_id = next_bp_id;
                            next_bp_id += 1;
                            bp_table.insert(*line, bp_id);
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
                    log::info!("breakpoints set for program {}: {:?}", program_id, bp_table);
                    log::info!("responding with {} breakpoints", bp_list.len());

                    server.respond(req.success(
                        ResponseBody::SetBreakpoints(SetBreakpointsResponse {
                            breakpoints: bp_list,
                        })
                    ))?;
                }
            }
            Command::RestartFrame(args) => {
                log::info!("restartFrame: frame_id={}", args.frame_id);

                // Resolve hart_id from frame_id mapping; fall back to
                // selected_hart_id if the frame is unknown (e.g. stale id).
                let restart_hart_id = frame_to_hart
                    .get(&args.frame_id)
                    .copied()
                    .unwrap_or(selected_hart_id);
                log::info!("restartFrame: hart_id={}", restart_hart_id);

                if let Some(ref mut m) = machine {
                    match m.init_hart_to_entry_point(restart_hart_id) {
                        Ok(()) => {
                            if let Some(hart) = m.get_hart(restart_hart_id) {
                                if let Some(line) = offset_to_line_map
                                    .get(&hart.program_id)
                                    .and_then(|otl| otl.get(&hart.pc))
                                {
                                    current_line = *line;
                                } else {
                                    current_line = 1;
                                }
                            }
                            log::info!("restartFrame: reset to line {}", current_line);
                            // P0-3: Clear stale frame_id mappings after hart reset.
                            // All prior StackTrace frame_ids now point to garbage state.
                            frame_to_hart.clear();
                            // P1-19: Send StoppedEvent BEFORE the response so
                            // VSCode correctly transitions to stopped state and
                            // refreshes StackTrace/Scopes.  All other state-
                            // changing operations (Goto, StepIn, Next, Continue)
                            // follow this pattern — RestartFrame was the only
                            // one that didn't.
                            server.send_event(Event::Stopped(StoppedEventBody {
                                reason: types::StoppedEventReason::Pause,
                                description: Some("frame restarted".into()),
                                thread_id: Some((restart_hart_id as i64) + 1),
                                preserve_focus_hint: Some(false),
                                text: None,
                                all_threads_stopped: Some(true),
                                hit_breakpoint_ids: Some(vec![]),
                            }))?;
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
                    thread_id: Some((selected_hart_id as i64) + 1),
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
                // Non-zero exit code so automation/scripts can
                // distinguish user-initiated termination (1) from
                // normal exit (0) and execution errors.
                server.send_event(Event::Exited(ExitedEventBody {
                    exit_code: 1,
                }))?;
                server.send_event(Event::Terminated(None))?;
                break;
            }
            Command::ConfigurationDone => {
                log::info!("configurationDone");

                server.respond(req.success(ResponseBody::ConfigurationDone))?;

                // Do NOT auto-set breakpoints on all lines — that would make
                // Continue behave like single-step (it would stop at every
                // source line). The initial StoppedEvent at the entry point
                // already gives the user a starting state; breakpoints are
                // user-controlled.

                // Respond to ConfigurationDone BEFORE sending StoppedEvent — stream ordering
                // guarantees VSCode processes the response first, then the event.
                // P0-12: Use selected_hart_id to derive the thread_id instead of
                // hardcoding 1.  In multi-hart scenarios the default hart may not
                // be hart 0, and the hardcoded 1 would map to the wrong thread.
                server.send_event(Event::Stopped(
                    StoppedEventBody {
                        reason: types::StoppedEventReason::Entry,
                        description: Some("entry".into()),
                        thread_id: Some((selected_hart_id as i64) + 1),
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
    /// Program ended normally (PC out of range)
    Terminated,
    /// Program ended due to execution error (illegal instruction, memory fault, etc.)
    Error(String),
}

/// Find the source line for the nearest mapped offset ≤ `offset`.
/// This is used when the PC lands at an address without direct debug info
/// (e.g. jump targets inside macro expansions). Returns the most recently
/// preceding source line so the UI doesn't show a stale/incorrect position.
fn find_nearest_line(
    offset_to_line_map: &HashMap<usize, HashMap<usize, usize>>,
    program_id: usize,
    offset: usize,
) -> Option<usize> {
    offset_to_line_map
        .get(&program_id)
        .and_then(|otl| {
            otl.iter()
                .filter(|(&o, _)| o <= offset)
                .max_by_key(|(&o, _)| o)
                .map(|(_, &line)| line)
        })
}

/// Execute one instruction on given hart and update current_line.
/// Shared by StepIn and Next — both do the same thing for RISC-V assembly.
fn do_single_step(
    machine: &mut Option<Machine>,
    hart_id: u64,
    current_line: &mut usize,
    offset_to_line_map: &HashMap<usize, HashMap<usize, usize>>,
) -> StepOutcome {
    if let Some(ref mut m) = machine {
        match m.step_hart(hart_id) {
            Ok(()) => {
                if let Some(hart) = m.get_hart(hart_id) {
                    log::info!("after step: t1(x6)={:x}, t2(x7)={:x}, t0(x5)={:x}, a0(x10)={:x}, a1(x11)={:x}, a2(x12)={:x}, pc={}",
                        hart.x.read(6), hart.x.read(7), hart.x.read(5),
                        hart.x.read(10), hart.x.read(11), hart.x.read(12),
                        hart.pc);
                    // Use instruction cache to check if PC is valid, not offset_to_line.
                    // offset_to_line only contains instructions with debug line info,
                    // so instructions without name_location would be incorrectly
                    // treated as program-termination.
                    if m.has_inst_at_pc(hart_id) {
                        // P0-2: Always update current_line after a step, even when the
                        // target address has no direct source line mapping. Fall back to
                        // the nearest preceding mapped offset so the debugger UI doesn't
                        // show a line that PC has already left behind.
                        let prog_id = hart.program_id;
                        let pc = hart.pc;
                        if let Some(line) = offset_to_line_map
                            .get(&prog_id)
                            .and_then(|otl| otl.get(&pc))
                        {
                            *current_line = *line;
                        } else if let Some(nearest) = find_nearest_line(offset_to_line_map, prog_id, pc) {
                            *current_line = nearest;
                        }
                        return StepOutcome::Ok;
                    } else {
                        log::info!("program finished (pc {} out of range)", hart.pc);
                    }
                }
            }
            Err(e) => {
                // P0-4: Propagate execution errors so callers can distinguish
                // normal termination from fatal errors (illegal instruction,
                // memory access fault, etc.). Previously the error was only
                // logged and discarded, making the UI silently exit.
                log::info!("step error: {:?}", e);
                let err_msg = m.get_hart(hart_id)
                    .and_then(|h| h.error_info.clone())
                    .unwrap_or_else(|| format!("{:?}", e));
                return StepOutcome::Error(err_msg);
            }
        }
    }
    StepOutcome::Terminated
}

/// Load and assemble a .rv.s file into a Machine and build per-program offset→line mapping
/// Normalize a file path for consistent HashMap lookups across DAP requests.
/// Uses std::fs::canonicalize which resolves symlinks, "..", "." segments
/// and normalizes the path to the OS-native form.  On Windows this also
/// normalizes drive letter case to uppercase (matching Windows convention).
///
/// Falls back to the original path if canonicalization fails (e.g. file
/// doesn't exist yet or permission denied).
fn canonicalize_path(path: &str) -> String {
    std::fs::canonicalize(path)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| path.to_string())
}

fn load_and_assemble(
    file_path: &str,
    offset_to_line_map: &mut HashMap<usize, HashMap<usize, usize>>,
    line_to_offsets: &mut HashMap<usize, HashMap<usize, Vec<usize>>>,
    source_to_program: &mut HashMap<String, usize>,
) -> Result<Machine, Box<dyn std::error::Error>> {
    log::info!("loading file: {}", file_path);

    let content = std::fs::read_to_string(file_path)
        .map_err(|e| format!("failed to read file {}: {}", file_path, e))?;

    log::info!("file content read, {} bytes", content.len());

    let program = parse_asm_use_default_config(&content)
        .map_err(|e| format!("assembly failed: {:?}", e))?;

    log::info!("assembly successful, {} text section items", 
        program.get_text_section_items().len());

    let mut machine = Machine::new();
    let program_id = machine.add_program(program)
        .map_err(|e| format!("failed to add program: {:?}", e))?;

    // Load the program into processor 0's local memory and update hart program_ids
    machine.load_program_to_processor(program_id, 0, true)
        .map_err(|e| format!("failed to load program into processor 0: {:?}", e))?;

    // Build offset → source line number mapping from instruction positions.
    // P0-18: Pseudo-instructions (e.g. `la`) expand to multiple machine-code
    // words.  We must map EVERY intermediate offset to the source line, not just
    // the first one.  Otherwise breakpoint detection in the Continue loop fails
    // when the PC lands on an intermediate offset (e.g. after a manual Goto).
    let mut offset_to_line: HashMap<usize, usize> = HashMap::new();
    let mut line_to_offs: HashMap<usize, Vec<usize>> = HashMap::new();
    let prog = &machine.programs[program_id];
    let labels = prog.get_labels();
    for item in prog.get_text_section_items() {
        if let Some(inst) = item.get_inc() {
            // Try to get the source line from the instruction's name location
            if let Some(range) = inst.get_name_location() {
                // SourceRange uses 1-based line numbers
                let line = range.start.line as usize;
                let base_offset = item.get_offset();
                let machine_codes = prog
                    .get_machine_code_list(item, &machine.registers, &labels)
                    .unwrap_or_default();
                let count = machine_codes.len().max(1);
                for i in 0..count {
                    let offset = base_offset + i * PC_INCREMENT;
                    offset_to_line.insert(offset, line);
                    line_to_offs.entry(line).or_default().push(offset);
                    log::info!("  offset {} -> source line {}", offset, line);
                }
            }
        }
    }
    // Sort offsets within each line for deterministic stepping
    for offsets in line_to_offs.values_mut() {
        offsets.sort_unstable();
    }
    log::info!("offset_to_line mapping for program {}: {:?}", program_id, offset_to_line);
    log::info!("line_to_offsets mapping for program {}: {:?}", program_id, line_to_offs);
    offset_to_line_map.insert(program_id, offset_to_line);
    line_to_offsets.insert(program_id, line_to_offs);

    // Build source path → program_id mapping so SetBreakpoints/GotoTargets
    // can match DAP source.path to the correct program.
    // P0-27: Canonicalize the path so subsequent DAP requests (which may send
    // slightly different path representations due to VSCode's internal
    // path normalization) always match — otherwise SetBreakpoints,
    // GotoTargets, and BreakpointLocations all fail simultaneously.
    let canonical_path = canonicalize_path(file_path);
    source_to_program.insert(canonical_path, program_id);

    // init hart to entry point
    machine.init_hart_to_entry_point(0)
        .map_err(|e| format!("failed to init hart: {:?}", e))?;

    log::info!("machine initialized, hart 0 pc = {:?}", 
        machine.get_hart(0).map(|h| h.pc));

    Ok(machine)
}
