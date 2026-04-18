use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

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
                name: "Rust Keyword LSP Server".to_string(),
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
            .log_message(MessageType::INFO, "Rust keyword LSP initialized")
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
        let mut prev_line: u32 = 0;
        let mut prev_start: u32 = 0;

        for (line_index, line) in text.lines().enumerate() {
            let mut current_col: usize = 0;

            for word in line.split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_') {
                if word.is_empty() {
                    continue;
                }

                if let Some(pos) = line[current_col..].find(word) {
                    let start_col = current_col + pos;
                    current_col = start_col + word.len();

                    if !KEYWORDS.contains(&word) {
                        continue;
                    }

                    let line_u32 = line_index as u32;
                    let start_u32 = start_col as u32;
                    let delta_line = line_u32 - prev_line;
                    let delta_start = if delta_line == 0 {
                        start_u32 - prev_start
                    } else {
                        start_u32
                    };

                    data.push(SemanticToken {
                        delta_line,
                        delta_start,
                        length: word.len() as u32,
                        token_type: 0,
                        token_modifiers_bitset: 0,
                    });

                    prev_line = line_u32;
                    prev_start = start_u32;
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
