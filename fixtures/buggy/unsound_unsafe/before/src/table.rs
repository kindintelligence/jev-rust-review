pub struct Table {
    values: Vec<u32>,
}

impl Table {
    pub fn new(values: Vec<u32>) -> Self {
        Self { values }
    }

    pub fn lookup(&self, index: usize) -> Option<u32> {
        self.values.get(index).copied()
    }
}
