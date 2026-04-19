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

#[test]
fn cached_document_state_returns_diagnostics_without_reparsing() {
    let state = DocumentState::from_text("bad <<".to_string());
    let diagnostics = state.diagnostics();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].severity, Some(DiagnosticSeverity::ERROR));
}

#[test]
fn cached_document_state_builds_semantic_tokens_from_stored_text() {
    let state = DocumentState::from_text("fn label".to_string());
    let tokens = state.semantic_tokens();

    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].length, 2);
}
