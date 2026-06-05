use super::hart::Hart;

pub struct Processor {
    pub id: u64,
    pub harts: Vec<Hart>,
}
