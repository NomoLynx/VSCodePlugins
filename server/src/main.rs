pub(crate) mod asm_parser;
pub(crate) mod lsp_location;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use asm_parser::asm_parse;
use lsp_location::*;
use riscv_asm_lib::r5asm::{
    asm_error::{AsmError, AsmErrorSourceFileLocation},
    asm_program::AsmProgram,
    instruction::SourceRange,
    register::Register,
};

// const string for LSP name 
const LSP_NAME: &str = "Rust RiscV LSP";

const TOKEN_TYPE_INSTRUCTION: u32 = 0;
const TOKEN_TYPE_REGISTER_ORDINARY: u32 = 1;
const TOKEN_TYPE_REGISTER_FLOAT: u32 = 2;
const TOKEN_TYPE_REGISTER_VECTOR: u32 = 3;
const TOKEN_TYPE_IMMEDIATE: u32 = 4;
const TOKEN_LEGEND_INSTRUCTION: &str = "instruction";
const TOKEN_LEGEND_REGISTER_ORDINARY: &str = "registerOrdinary";
const TOKEN_LEGEND_REGISTER_FLOAT: &str = "registerFloat";
const TOKEN_LEGEND_REGISTER_VECTOR: &str = "registerVector";
const TOKEN_LEGEND_IMMEDIATE: &str = "immediate";

static TOKEN_TEXT_DEBUG_MESSAGES: OnceLock<Mutex<Vec<String>>> = OnceLock::new();

#[derive(Debug)]
enum ParseState {
    Parsed(AsmProgram),
    Error(AsmError),
    Panic(String),
}

#[derive(Debug)]
struct DocumentState {
    text: String,
    parse_state: ParseState,
}

impl DocumentState {
    fn from_text(text: String) -> Self {
        let parse_state = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| asm_parse(&text))) {
            Ok(Ok(program)) => ParseState::Parsed(program),
            Ok(Err(err)) => ParseState::Error(err),
            Err(_) => ParseState::Panic("Internal parser panic while producing diagnostics".to_string()),
        };

        Self { text, parse_state }
    }

    fn diagnostics(&self) -> Vec<Diagnostic> {
        match &self.parse_state {
            ParseState::Parsed(_) => Vec::new(),
            ParseState::Error(err) => vec![build_error_diagnostic(&self.text, err)],
            ParseState::Panic(message) => vec![Diagnostic {
                range: fallback_diagnostic_range(&self.text, Some(message)),
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some(LSP_NAME.to_string()),
                message: message.clone(),
                ..Diagnostic::default()
            }],
        }
    }

    fn semantic_tokens(&self) -> Vec<SemanticToken> {
        match &self.parse_state {
            ParseState::Parsed(program) => collect_semantic_tokens(&self.text, program),
            ParseState::Error(_) | ParseState::Panic(_) => Vec::new(),
        }
    }
}

#[derive(Debug)]
struct Backend {
    client: Client,
    documents: Arc<RwLock<HashMap<Url, DocumentState>>>,
}

impl Backend {
    fn new(client: Client) -> Self {
        Self {
            client,
            documents: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn update_document(&self, uri: Url, text: String) {
        let state = DocumentState::from_text(text);

        let mut documents = self.documents.write().await;
        documents.insert(uri, state);
    }

    async fn publish_parse_diagnostics(&self, uri: Url) {
        let diagnostics = {
            let documents = self.documents.read().await;
            documents
                .get(&uri)
                .map(DocumentState::diagnostics)
                .unwrap_or_default()
        };

        self.client.publish_diagnostics(uri, diagnostics, None).await;
    }
}

#[cfg(test)]
fn collect_parse_diagnostics(text: &str) -> Vec<Diagnostic> {
    DocumentState::from_text(text.to_string()).diagnostics()
}

fn token_text_debug_messages() -> &'static Mutex<Vec<String>> {
    TOKEN_TEXT_DEBUG_MESSAGES.get_or_init(|| Mutex::new(Vec::new()))
}

fn push_token_text_debug_message(message: String) {
    if let Ok(mut messages) = token_text_debug_messages().lock() {
        messages.push(message);
    }
}

fn take_token_text_debug_messages() -> Vec<String> {
    if let Ok(mut messages) = token_text_debug_messages().lock() {
        std::mem::take(&mut *messages)
    } else {
        Vec::new()
    }
}

fn collect_semantic_tokens(text: &str, program: &AsmProgram) -> Vec<SemanticToken> {
    let mut ranges = Vec::new();

    for item in program.get_text_section_items() {
        let Some(instruction) = item.get_inc() else {
            continue;
        };

        if let Some(range) = instruction.get_name_location().copied() {
            ranges.push((range, TOKEN_TYPE_INSTRUCTION));
        }

        for register_range in [
            instruction.get_r0_location().copied(),
            instruction.get_r1_location().copied(),
            instruction.get_r2_location().copied(),
            instruction.get_r3_location().copied(),
        ]
        .into_iter()
        .flatten()
        {
            let token_type = token_text_from_range(text, &register_range)
                .map(|register_name| classify_register_token(&register_name))
                .unwrap_or(TOKEN_TYPE_REGISTER_ORDINARY);
            ranges.push((register_range, token_type));
        }

        if let Some(range) = instruction.get_imm_location().copied() {
            ranges.push((range, TOKEN_TYPE_IMMEDIATE));
        }
    }

    ranges.sort_by_key(|(range, _)| {
        (
            range.start.line,
            range.start.column,
            range.end.line,
            range.end.column,
        )
    });

    let mut data: Vec<SemanticToken> = Vec::with_capacity(ranges.len());
    let mut prev_location = LSPLocation::default();

    for (range, token_type) in ranges {
        if let Some(token) = semantic_token_from_range(&range, &mut prev_location, token_type) {
            data.push(token);
        }
    }

    data
}

fn token_text_from_range(text: &str, range: &SourceRange) -> Option<String> {
    if range.start.line == 0 || range.start.column == 0 {
        push_token_text_debug_message(format!(
            "token_text_from_range skipped invalid start position: {:?}",
            range
        ));
        return None;
    }

    let Some(line) = text.lines().nth(range.start.line.saturating_sub(1)) else {
        push_token_text_debug_message(format!(
            "token_text_from_range could not find line {} for range {:?}",
            range.start.line,
            range
        ));
        return None;
    };
    let start_column = range.start.column.saturating_sub(1);
    let length = range.end.column.saturating_sub(range.start.column);

    if length == 0 {
        push_token_text_debug_message(format!(
            "token_text_from_range skipped zero-length range: {:?}",
            range
        ));
        return None;
    }

    let token_text: String = line
        .chars()
        .skip(start_column)
        .take(length)
        .collect();

    if token_text.is_empty() {
        push_token_text_debug_message(format!(
            "token_text_from_range produced empty text for range {:?}",
            range
        ));
        None
    } else {
        push_token_text_debug_message(format!(
            "token_text_from_range {:?} -> '{}'",
            range,
            token_text
        ));
        Some(token_text)
    }
}

fn classify_register_token(register_name: &str) -> u32 {
    if Register::is_float_register_name(register_name) {
        TOKEN_TYPE_REGISTER_FLOAT
    } else if Register::is_vector_register_name(register_name) {
        TOKEN_TYPE_REGISTER_VECTOR
    } else {
        TOKEN_TYPE_REGISTER_ORDINARY
    }
}

fn semantic_token_from_range(
    range: &SourceRange,
    prev_location: &mut LSPLocation,
    token_type: u32,
) -> Option<SemanticToken> {
    if range.start.line == 0 || range.start.column == 0 {
        return None;
    }

    let token_location = LSPLocation {
        line: range.start.line.saturating_sub(1) as u32,
        character: range.start.column.saturating_sub(1) as u32,
    };
    let length = range.end.column.saturating_sub(range.start.column) as u32;

    if length == 0 {
        return None;
    }

    let delta = token_location - *prev_location;
    *prev_location = token_location;

    Some(SemanticToken {
        delta_line: delta.line,
        delta_start: delta.character,
        length,
        token_type,
        token_modifiers_bitset: 0,
    })
}

fn build_error_diagnostic(text: &str, err: &AsmError) -> Diagnostic {
    let mut message = err.get_error_message();
    let range = match err.get_error_location() {
        Some(location) => {
            message.push_str(&format!("\nError location: {location}"));
            diagnostic_range_from_error_location(text, location)
        }
        None => {
            message.push_str("\nNo AsmErrorSourceFileLocation was available.");
            fallback_diagnostic_range(text, Some(&message))
        }
    };

    Diagnostic {
        range,
        severity: Some(DiagnosticSeverity::ERROR),
        source: Some(LSP_NAME.to_string()),
        message,
        ..Diagnostic::default()
    }
}

fn diagnostic_range_from_error_location(
    text: &str,
    location: &AsmErrorSourceFileLocation,
) -> Range {
    let line_count = text.lines().count().max(1) as u32;
    let safe_line = location
        .1
        .saturating_sub(1)
        .min(line_count.saturating_sub(1));
    let line_len = text
        .lines()
        .nth(safe_line as usize)
        .map(|line_text| line_text.chars().count() as u32)
        .unwrap_or(1)
        .max(1);

    Range {
        start: Position::new(safe_line, 0),
        end: Position::new(safe_line, line_len),
    }
}

fn fallback_diagnostic_range(text: &str, message: Option<&str>) -> Range {
    if let Some((line, character)) = message.and_then(parse_line_and_character) {
        let first_line_len = text
            .lines()
            .nth(line as usize)
            .map(|line_text| line_text.chars().count() as u32)
            .unwrap_or(character.saturating_add(1))
            .max(character.saturating_add(1));

        return Range {
            start: Position::new(line, character),
            end: Position::new(line, first_line_len),
        };
    }

    let first_line_len = text
        .lines()
        .next()
        .map(|line_text| line_text.chars().count() as u32)
        .unwrap_or(1)
        .max(1);

    Range {
        start: Position::new(0, 0),
        end: Position::new(0, first_line_len),
    }
}

fn parse_line_and_character(message: &str) -> Option<(u32, u32)> {
    for marker in ["Pos((", "Span(("] {
        if let Some(start) = message.find(marker) {
            let numbers = message[start + marker.len()..]
                .split(|ch: char| !ch.is_ascii_digit())
                .filter(|part| !part.is_empty())
                .take(2)
                .filter_map(|part| part.parse::<u32>().ok())
                .collect::<Vec<_>>();

            if numbers.len() == 2 {
                return Some((numbers[0].saturating_sub(1), numbers[1].saturating_sub(1)));
            }
        }
    }

    None
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        let legend = SemanticTokensLegend {
            token_types: vec![
                SemanticTokenType::new(TOKEN_LEGEND_INSTRUCTION),
                SemanticTokenType::new(TOKEN_LEGEND_REGISTER_ORDINARY),
                SemanticTokenType::new(TOKEN_LEGEND_REGISTER_FLOAT),
                SemanticTokenType::new(TOKEN_LEGEND_REGISTER_VECTOR),
                SemanticTokenType::new(TOKEN_LEGEND_IMMEDIATE),
            ],
            token_modifiers: vec![],
        };

        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: LSP_NAME.to_string(),
                version: Some("0.1.0".to_string()),
            }),
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            legend,
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                            range: None,
                            work_done_progress_options: Default::default(),
                        },
                    ),
                ),
                ..ServerCapabilities::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, format!("{LSP_NAME} initialized"))
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;

        self.update_document(uri.clone(), text).await;
        self.publish_parse_diagnostics(uri).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.into_iter().next() {
            let uri = params.text_document.uri;
            let text = change.text;

            self.update_document(uri.clone(), text).await;
            self.publish_parse_diagnostics(uri).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;

        {
            let mut documents = self.documents.write().await;
            documents.remove(&uri);
        }

        self.client.publish_diagnostics(uri, Vec::new(), None).await;
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let data = {
            let documents = self.documents.read().await;
            let Some(state) = documents.get(&params.text_document.uri) else {
                return Ok(None);
            };

            state.semantic_tokens()
        };

        for message in take_token_text_debug_messages() {
            self.client.log_message(MessageType::INFO, message).await;
        }

        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data,
        })))
    }
}

#[cfg(test)]
mod tests;

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
