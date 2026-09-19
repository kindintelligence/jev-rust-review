pub struct Table {
    values: Vec<u32>,
}

impl Table {
    pub fn new(values: Vec<u32>) -> Self {
        Self { values }
    }

    /// Fast path used by the hot loop.
    pub fn lookup(&self, index: usize) -> u32 {
        unsafe { *self.values.get_unchecked(index) }
    }
}
