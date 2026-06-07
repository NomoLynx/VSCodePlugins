use super::hart::Hart;

pub struct Processor {
    pub id: u64,
    pub harts: Vec<Hart>,
}

impl Default for Processor {
    fn default() -> Self {
        Self {
            id: 0,
            harts: vec![Hart::default()],
        }
    }
}
