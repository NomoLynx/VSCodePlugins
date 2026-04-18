#[derive(Debug, Clone, Copy, Default)]
pub struct LSPLocation {
    pub(crate) line: u32,
    pub(crate) character: u32,
}

/// implement operator - for LSPLocation
/// the result is the delta between two locations, which is used for semantic tokens
impl std::ops::Sub for LSPLocation {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        
        Self {
            line: self.line - other.line,
            character: if self.line == other.line { self.character - other.character } 
                       else { self.character },
        }
    }
}