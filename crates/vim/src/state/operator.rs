use super::*;

impl Operator {
    pub fn id(&self) -> &'static str {
        match self {
            Operator::Object { around: false } => "i",
            Operator::Object { around: true } => "a",
            Operator::Change => "c",
            Operator::Delete => "d",
            Operator::Yank => "y",
            Operator::Replace => "r",
            Operator::Digraph { .. } => "^K",
            Operator::Literal { .. } => "^V",
            Operator::FindForward { before: false, .. } => "f",
            Operator::FindForward { before: true, .. } => "t",
            Operator::Sneak { .. } => "s",
            Operator::SneakBackward { .. } => "S",
            Operator::FindBackward { after: false, .. } => "F",
            Operator::FindBackward { after: true, .. } => "T",
            Operator::AddSurrounds { .. } => "ys",
            Operator::ChangeSurrounds { .. } => "cs",
            Operator::DeleteSurrounds => "ds",
            Operator::Mark => "m",
            Operator::Jump { line: true } => "'",
            Operator::Jump { line: false } => "`",
            Operator::Indent => ">",
            Operator::AutoIndent => "eq",
            Operator::ShellCommand => "sh",
            Operator::Rewrap => "gq",
            Operator::ReplaceWithRegister => "gR",
            Operator::Exchange => "cx",
            Operator::Outdent => "<",
            Operator::Uppercase => "gU",
            Operator::Lowercase => "gu",
            Operator::OppositeCase => "g~",
            Operator::Rot13 => "g?",
            Operator::Rot47 => "g?",
            Operator::Register => "\"",
            Operator::RecordRegister => "q",
            Operator::ReplayRegister => "@",
            Operator::ToggleComments => "gc",
            Operator::ToggleBlockComments => "gb",
            Operator::HelixMatch => "helix_m",
            Operator::HelixNext { .. } => "helix_next",
            Operator::HelixPrevious { .. } => "helix_previous",
            Operator::HelixJump { .. } => "gw",
            Operator::HelixSurroundAdd => "helix_ms",
            Operator::HelixSurroundReplace { .. } => "helix_mr",
            Operator::HelixSurroundDelete => "helix_md",
        }
    }

    pub fn status(&self) -> String {
        fn make_visible(c: &str) -> &str {
            match c {
                "\n" => "enter",
                "\t" => "tab",
                " " => "space",
                c => c,
            }
        }
        match self {
            Operator::Digraph {
                first_char: Some(first_char),
            } => format!("^K{}", make_visible(&first_char.to_string())),
            Operator::Literal {
                prefix: Some(prefix),
            } => format!("^V{}", make_visible(prefix)),
            Operator::AutoIndent => "=".to_string(),
            Operator::ShellCommand => "=".to_string(),
            Operator::HelixMatch => "m".to_string(),
            Operator::HelixNext { .. } => "]".to_string(),
            Operator::HelixPrevious { .. } => "[".to_string(),
            Operator::HelixJump { .. } => "gw".to_string(),
            Operator::HelixSurroundAdd => "ms".to_string(),
            Operator::HelixSurroundReplace {
                replaced_char: None,
            } => "mr".to_string(),
            Operator::HelixSurroundReplace {
                replaced_char: Some(c),
            } => format!("mr{}", c),
            Operator::HelixSurroundDelete => "md".to_string(),
            _ => self.id().to_string(),
        }
    }

    pub fn is_waiting(&self, mode: Mode) -> bool {
        match self {
            Operator::AddSurrounds { target } => target.is_some() || mode.is_visual(),
            Operator::FindForward { .. }
            | Operator::Mark
            | Operator::Jump { .. }
            | Operator::FindBackward { .. }
            | Operator::Sneak { .. }
            | Operator::SneakBackward { .. }
            | Operator::Register
            | Operator::RecordRegister
            | Operator::ReplayRegister
            | Operator::Replace
            | Operator::Digraph { .. }
            | Operator::Literal { .. }
            | Operator::ChangeSurrounds {
                target: Some(_), ..
            }
            | Operator::DeleteSurrounds
            | Operator::HelixJump { .. } => true,
            Operator::Change
            | Operator::Delete
            | Operator::Yank
            | Operator::Rewrap
            | Operator::Indent
            | Operator::Outdent
            | Operator::AutoIndent
            | Operator::ShellCommand
            | Operator::Lowercase
            | Operator::Uppercase
            | Operator::Rot13
            | Operator::Rot47
            | Operator::ReplaceWithRegister
            | Operator::Exchange
            | Operator::Object { .. }
            | Operator::ChangeSurrounds { target: None, .. }
            | Operator::OppositeCase
            | Operator::ToggleComments
            | Operator::ToggleBlockComments
            | Operator::HelixMatch
            | Operator::HelixNext { .. }
            | Operator::HelixPrevious { .. } => false,
            Operator::HelixSurroundAdd
            | Operator::HelixSurroundReplace { .. }
            | Operator::HelixSurroundDelete => true,
        }
    }

    pub fn starts_dot_recording(&self) -> bool {
        match self {
            Operator::Change
            | Operator::Delete
            | Operator::Replace
            | Operator::Indent
            | Operator::Outdent
            | Operator::AutoIndent
            | Operator::Lowercase
            | Operator::Uppercase
            | Operator::OppositeCase
            | Operator::Rot13
            | Operator::Rot47
            | Operator::ToggleComments
            | Operator::ToggleBlockComments
            | Operator::ReplaceWithRegister
            | Operator::Rewrap
            | Operator::ShellCommand
            | Operator::AddSurrounds { target: None }
            | Operator::ChangeSurrounds { target: None, .. }
            | Operator::DeleteSurrounds
            | Operator::Exchange
            | Operator::HelixNext { .. }
            | Operator::HelixPrevious { .. }
            | Operator::HelixSurroundAdd
            | Operator::HelixSurroundReplace { .. }
            | Operator::HelixSurroundDelete => true,
            Operator::Yank
            | Operator::Object { .. }
            | Operator::FindForward { .. }
            | Operator::FindBackward { .. }
            | Operator::Sneak { .. }
            | Operator::SneakBackward { .. }
            | Operator::Mark
            | Operator::Digraph { .. }
            | Operator::Literal { .. }
            | Operator::AddSurrounds { .. }
            | Operator::ChangeSurrounds { .. }
            | Operator::Jump { .. }
            | Operator::Register
            | Operator::RecordRegister
            | Operator::ReplayRegister
            | Operator::HelixMatch
            | Operator::HelixJump { .. } => false,
        }
    }
}
