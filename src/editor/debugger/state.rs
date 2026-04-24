#[derive(Clone, Debug, Default)]
pub struct ThreadSummary {
    pub id: i64,
    pub name: String,
}

#[derive(Clone, Debug, Default)]
pub struct StackFrameSummary {
    pub id: i64,
    pub name: String,
    pub line: i64,
    pub column: i64,
    pub source_path: String,
}

#[derive(Clone, Debug, Default)]
pub struct VariableSummary {
    pub name: String,
    pub value: String,
    pub type_name: String,
    pub variables_reference: i64,
}

#[derive(Clone, Debug, Default)]
pub struct DebugState {
    pub active: bool,
    pub current_thread_id: Option<i64>,
    pub threads: Vec<ThreadSummary>,
    pub stack_frames: Vec<StackFrameSummary>,
    pub variables: Vec<VariableSummary>,
}
