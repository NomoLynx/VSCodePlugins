#[derive(Clone, Debug)]
pub enum Inst {
    Add,
    Addi,
    FAddD,
    VAddVV,
}

#[derive(Clone, Debug)]
pub struct SourceInst {
    pub line: usize,
    pub text: String,
    pub inst_range: (usize, usize),
}

pub struct Program {
    pub instructions: Vec<Inst>,
    pub source_map: Vec<SourceInst>,
}
