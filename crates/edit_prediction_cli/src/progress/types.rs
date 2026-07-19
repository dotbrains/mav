#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Step {
    LoadProject,
    Context,
    FormatPrompt,
    Predict,
    Score,
    Qa,
    Repair,
    Synthesize,
    PullExamples,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InfoStyle {
    Normal,
    Warning,
}

impl Step {
    pub fn label(&self) -> &'static str {
        match self {
            Step::LoadProject => "Load",
            Step::Context => "Context",
            Step::FormatPrompt => "Format",
            Step::Predict => "Predict",
            Step::Score => "Score",
            Step::Qa => "QA",
            Step::Repair => "Repair",
            Step::Synthesize => "Synthesize",
            Step::PullExamples => "Pull",
        }
    }

    pub(super) fn color_code(&self) -> &'static str {
        match self {
            Step::LoadProject => "\x1b[33m",
            Step::Context => "\x1b[35m",
            Step::FormatPrompt => "\x1b[34m",
            Step::Predict => "\x1b[32m",
            Step::Score => "\x1b[31m",
            Step::Qa => "\x1b[36m",
            Step::Repair => "\x1b[95m",
            Step::Synthesize => "\x1b[36m",
            Step::PullExamples => "\x1b[36m",
        }
    }
}
