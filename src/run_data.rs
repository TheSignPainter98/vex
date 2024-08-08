use crate::irritation::Irritation;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct RunData {
    pub irritations: Vec<Irritation>,
    pub num_files_scanned: usize,
}

#[cfg(test)]
impl RunData {
    pub fn into_irritations(self) -> Vec<Irritation> {
        self.irritations
    }
}
