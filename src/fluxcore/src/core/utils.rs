pub trait Ignore : Sized {
    fn ignore(self) {}
}

impl<T> Ignore for Option<T> {}

impl<R, E> Ignore for Result<R, E> {}

pub struct ProgressTracker {
    cur: u64,
    max: u64,
}

impl ProgressTracker {
    pub fn new(max: u64) -> Self {
        Self{ cur: 0, max }
    }

    pub fn percent(&self) -> f64 {
        self.cur as f64 / self.max as f64
    }

    pub fn inc(&mut self) {
        self.cur += 1;
    }

    pub fn cur(&self) -> u64 {
        self.cur
    }

    pub fn max(&self) -> u64 {
        self.max
    }
}