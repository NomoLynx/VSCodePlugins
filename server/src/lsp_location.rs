#[derive(Debug, Clone, Copy, Default)]
pub struct LSPLocation {
    pub(crate) line: u32,
    pub(crate) character: u32,
}

/// Implements `Sub` for [`LSPLocation`].
///
/// The result is the delta between two locations, used for LSP semantic-token
/// delta encoding.  Uses [`saturating_sub`] to avoid panics on out-of-order
/// positions (defensive: callers should pass monotonically increasing locations).
impl std::ops::Sub for LSPLocation {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Self {
            line: self.line.saturating_sub(other.line),
            character: if self.line == other.line {
                self.character.saturating_sub(other.character)
            } else {
                self.character
            },
        }
    }
}