use super::*;

#[derive(Debug, PartialEq)]
pub(super) enum PromptCompletion {
    SlashCommand(SlashCommandCompletion),
    Mention(MentionCompletion),
}

impl PromptCompletion {
    fn source_range(&self) -> Range<usize> {
        match self {
            Self::SlashCommand(completion) => completion.source_range.clone(),
            Self::Mention(completion) => completion.source_range.clone(),
        }
    }

    fn try_parse(
        line: &str,
        offset_to_line: usize,
        supported_modes: &[PromptContextType],
    ) -> Option<Self> {
        if line.contains('@') {
            if let Some(mention) =
                MentionCompletion::try_parse(line, offset_to_line, supported_modes)
            {
                return Some(Self::Mention(mention));
            }
        }
        SlashCommandCompletion::try_parse(line, offset_to_line).map(Self::SlashCommand)
    }
}

#[derive(Debug, Default, PartialEq)]
pub struct SlashCommandCompletion {
    pub source_range: Range<usize>,
    pub command: Option<String>,
    pub argument: Option<String>,
}

impl SlashCommandCompletion {
    pub fn try_parse(line: &str, offset_to_line: usize) -> Option<Self> {
        let mut last_command_start = None;
        for (idx, _) in line.rmatch_indices('/') {
            if line[idx + 1..]
                .chars()
                .next()
                .is_some_and(|c| c.is_whitespace())
            {
                continue;
            }

            if idx > 0
                && line[..idx]
                    .chars()
                    .last()
                    .is_some_and(|c| !c.is_whitespace())
            {
                continue;
            }

            last_command_start = Some(idx);
            break;
        }

        let last_command_start = last_command_start?;
        let last_command = &line[last_command_start + 1..];

        let mut argument = None;
        let mut command = None;
        if let Some((command_text, args)) = last_command.split_once(char::is_whitespace) {
            if !args.is_empty() {
                argument = Some(args.trim_end().to_string());
            }
            command = Some(command_text.to_string());
        } else if !last_command.is_empty() {
            command = Some(last_command.to_string());
        };

        Some(Self {
            source_range: last_command_start + offset_to_line
                ..line
                    .rfind(|c: char| !c.is_whitespace())
                    .unwrap_or_else(|| line.len())
                    + 1
                    + offset_to_line,
            command,
            argument,
        })
    }
}

#[derive(Debug, Default, PartialEq)]
pub(super) struct MentionCompletion {
    source_range: Range<usize>,
    mode: Option<PromptContextType>,
    argument: Option<String>,
}

impl MentionCompletion {
    fn try_parse(
        line: &str,
        offset_to_line: usize,
        supported_modes: &[PromptContextType],
    ) -> Option<Self> {
        // Find the rightmost '@' that has a boundary before it and no whitespace immediately after.
        // A boundary is the start of the line, whitespace, or an opening bracket.
        let mut last_mention_start = None;
        for (idx, _) in line.rmatch_indices('@') {
            // No whitespace immediately after '@'.
            if line[idx + 1..]
                .chars()
                .next()
                .is_some_and(|c| c.is_whitespace())
            {
                continue;
            }

            if idx > 0
                && line[..idx]
                    .chars()
                    .last()
                    .is_some_and(|c| !c.is_whitespace() && !matches!(c, '(' | '[' | '{'))
            {
                continue;
            }

            last_mention_start = Some(idx);
            break;
        }

        let last_mention_start = last_mention_start?;

        let rest_of_line = &line[last_mention_start + 1..];

        let mut mode = None;
        let mut argument = None;

        let mut parts = rest_of_line.split_whitespace();
        let mut end = last_mention_start + 1;

        if let Some(mode_text) = parts.next() {
            // Safe since we check no leading whitespace above
            end += mode_text.len();

            if let Some(parsed_mode) = PromptContextType::try_from(mode_text).ok()
                && supported_modes.contains(&parsed_mode)
            {
                mode = Some(parsed_mode);
            } else {
                argument = Some(mode_text.to_string());
            }
            match rest_of_line[mode_text.len()..].find(|c: char| !c.is_whitespace()) {
                Some(whitespace_count) => {
                    if let Some(argument_text) = parts.next() {
                        // If mode wasn't recognized but we have an argument, don't suggest completions
                        // (e.g. '@something word')
                        if mode.is_none() && !argument_text.is_empty() {
                            return None;
                        }

                        argument = Some(argument_text.to_string());
                        end += whitespace_count + argument_text.len();
                    }
                }
                None => {
                    // Rest of line is entirely whitespace
                    end += rest_of_line.len() - mode_text.len();
                }
            }
        }

        Some(Self {
            source_range: last_mention_start + offset_to_line..end + offset_to_line,
            mode,
            argument,
        })
    }
}
