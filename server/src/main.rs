pub(crate) mod asm_parser;
pub(crate) mod lsp_location;

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use lsp_location::*;

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
        let mut docs = self.docs.write().await;
        docs.insert(params.text_document.uri, params.text_document.text);
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.into_iter().next() {
            let mut docs = self.docs.write().await;
            docs.insert(params.text_document.uri, change.text);
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let mut docs = self.docs.write().await;
        docs.remove(&params.text_document.uri);
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

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
