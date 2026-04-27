#[derive(Default, Debug)]
pub struct DocumentStatus {
    pub total_lines: usize,
    pub current_line_idx: usize,
    pub is_modified: bool,
    pub file_name: String,
    pub language_name: Option<String>,
    pub debug_state_label: Option<String>,
}

impl PartialEq for DocumentStatus {
    fn eq(&self, other: &Self) -> bool {
        self.total_lines == other.total_lines
            && self.current_line_idx == other.current_line_idx
            && self.is_modified == other.is_modified
            && self.file_name == other.file_name
            && self.language_name == other.language_name
            && self.debug_state_label == other.debug_state_label
    }
}

impl Eq for DocumentStatus {}

impl DocumentStatus {
    pub fn modified_indicator_to_string(&self) -> String {
        if self.is_modified {
            String::from("(modified)")
        } else {
            String::new()
        }
    }
    pub fn line_count_to_string(&self) -> String {
        format!("{} lines", self.total_lines)
    }
    pub fn position_indicator_to_string(&self) -> String {
        format!(
            "{}/{}",
            self.current_line_idx.saturating_add(1),
            self.total_lines
        )
    }
}
