pub(crate) mod asm_parser;
pub(crate) mod lsp_location;

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use asm_parser::asm_parse;
use lsp_location::*;
use riscv_asm_lib::r5asm::asm_error::{AsmError, AsmErrorSourceFileLocation};

// const string for LSP name 
const LSP_NAME: &str = "Rust RiscV LSP";

const KEYWORDS: &[&str] = &[
    "fn", "let", "mut", "if", "else", "return", "match", "while", "for", "in", "struct",
    "enum", "impl", "use", "pub", "mod", "trait", "const", "static", "as", "xxx", "yyy", "zzz",
];

#[derive(Debug)]
struct Backend {
    client: Client,
    docs: Arc<RwLock<HashMap<Url, String>>>,
}

impl Backend {
    fn new(client: Client) -> Self {
        Self {
            client,
            docs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn publish_parse_diagnostics(&self, uri: Url, text: String) {
        let diagnostics = collect_parse_diagnostics(&text);
        self.client.publish_diagnostics(uri, diagnostics, None).await;
    }
}

fn collect_parse_diagnostics(text: &str) -> Vec<Diagnostic> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| asm_parse(text))) {
        Ok(Ok(_)) => Vec::new(),
        Ok(Err(err)) => vec![build_error_diagnostic(text, err)],
        Err(_) => vec![Diagnostic {
            range: fallback_diagnostic_range(text, Some("Internal parser panic while producing diagnostics")),
            severity: Some(DiagnosticSeverity::ERROR),
            source: Some(LSP_NAME.to_string()),
            message: "Internal parser panic while producing diagnostics".to_string(),
            ..Diagnostic::default()
        }],
    }
}

fn build_error_diagnostic(text: &str, err: AsmError) -> Diagnostic {
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
            token_types: vec![SemanticTokenType::KEYWORD],
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
            .log_message(MessageType::INFO, format!("{LSP_NAME}initialized"))
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;

        {
            let mut docs = self.docs.write().await;
            docs.insert(uri.clone(), text.clone());
        }

        self.publish_parse_diagnostics(uri, text).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.into_iter().next() {
            let uri = params.text_document.uri;
            let text = change.text;

            {
                let mut docs = self.docs.write().await;
                docs.insert(uri.clone(), text.clone());
            }

            self.publish_parse_diagnostics(uri, text).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;

        {
            let mut docs = self.docs.write().await;
            docs.remove(&uri);
        }

        self.client.publish_diagnostics(uri, Vec::new(), None).await;
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let docs = self.docs.read().await;
        let Some(text) = docs.get(&params.text_document.uri) else {
            return Ok(None);
        };

        let mut data: Vec<SemanticToken> = Vec::new();
        let mut prev_location = LSPLocation::default();

        for (line_index, line) in text.lines().enumerate() {
            let mut search_location = LSPLocation {
                line: line_index as u32,
                character: 0,
            };

            for word in line.split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_') {
                if word.is_empty() {
                    continue;
                }

                if let Some(pos) = line[search_location.character as usize..].find(word) {
                    let token_location = LSPLocation {
                        line: line_index as u32,
                        character: search_location.character + pos as u32,
                    };
                    search_location.character = token_location.character + word.len() as u32;

                    if !KEYWORDS.contains(&word) {
                        continue;
                    }

                    let delta = token_location - prev_location;

                    data.push(SemanticToken {
                        delta_line: delta.line,
                        delta_start: delta.character,
                        length: word.len() as u32,
                        token_type: 0,
                        token_modifiers_bitset: 0,
                    });

                    prev_location = token_location;
                }
            }
        }

        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data,
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_position_from_pest_message() {
        let position = parse_line_and_character("error @ Pos((3, 7))").expect("should extract position");
        assert_eq!(position, (2, 6));
    }

    #[test]
    fn invalid_input_produces_error_diagnostic() {
        let diagnostics = collect_parse_diagnostics("bad <<");
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, Some(DiagnosticSeverity::ERROR));
        assert!(diagnostics[0].message.contains("Parsing") || diagnostics[0].message.contains("GeneralError"));
    }

    #[test]
    fn parser_message_line_six_maps_to_vscode_line_five() {
        let text = "a\nb\nc\nd\ne\nfn efg {\n";
        let location = AsmErrorSourceFileLocation("dummy.rs".to_string(), 6);

        let range = diagnostic_range_from_error_location(text, &location);

        assert_eq!(range.start.line, 5);
    }

    #[test]
    fn out_of_range_location_clamps_to_last_line() {
        let text = "a\nb\nc\n";
        let location = AsmErrorSourceFileLocation("dummy.rs".to_string(), 99);

        let range = diagnostic_range_from_error_location(text, &location);

        assert_eq!(range.start.line, 2);
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
