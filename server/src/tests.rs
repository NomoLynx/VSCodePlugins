use super::*;

fn decode_token_positions(tokens: &[SemanticToken]) -> Vec<(u32, u32, u32, u32)> {
    let mut line = 0;
    let mut start = 0;
    let mut positions = Vec::new();

    for token in tokens {
        line += token.delta_line;
        start = if token.delta_line == 0 {
            start + token.delta_start
        } else {
            token.delta_start
        };
        positions.push((line, start, token.length, token.token_type));
    }

    positions
}

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

#[test]
fn cached_document_state_returns_diagnostics_without_reparsing() {
    let state = DocumentState::from_text("bad <<".to_string());
    let diagnostics = state.diagnostics();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].severity, Some(DiagnosticSeverity::ERROR));
}

#[test]
fn semantic_tokens_use_instruction_and_register_source_ranges() {
    let state = DocumentState::from_text(".text\naddi x1, x2, 123".to_string());
    let tokens = state.semantic_tokens();
    let positions = decode_token_positions(&tokens);

    assert_eq!(positions, vec![(1, 0, 4, 0), (1, 5, 2, 1), (1, 9, 2, 1)]);
}

#[test]
fn semantic_tokens_highlight_label_line_with_distinct_register_type() {
    let state = DocumentState::from_text(".text\nloop: c.addi x1, 1".to_string());
    let tokens = state.semantic_tokens();
    let positions = decode_token_positions(&tokens);

    assert_eq!(positions, vec![(1, 6, 6, 0), (1, 13, 2, 1)]);
}

#[test]
fn invalid_input_does_not_break_semantic_tokens() {
    let state = DocumentState::from_text("bad <<".to_string());
    let tokens = state.semantic_tokens();

    assert!(tokens.is_empty());
}
