pub trait DebugAdapter {
    fn step(&mut self);
    fn continue_run(&mut self);
}
