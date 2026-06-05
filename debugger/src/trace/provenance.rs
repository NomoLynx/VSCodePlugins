pub enum Location {
    Register(u64, String),
    Memory(u64),
}

pub struct ValueNode {
    pub value_id: u64,
    pub location: Location,
}
