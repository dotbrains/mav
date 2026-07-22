use super::*;

pub fn command_interceptor(
    mut input: &str,
    workspace: WeakEntity<Workspace>,
    cx: &mut App,
) -> Task<CommandInterceptResult> {
    while input.starts_with(':') {
        input = &input[1..];
    }

    let (range, query) = VimCommand::parse_range(input);
    let range_prefix = input[0..(input.len() - query.len())].to_string();
    let has_trailing_space = query.ends_with(" ");
    let mut query = query.as_str().trim_start();

    let on_matching_lines = (query.starts_with('g') || query.starts_with('v'))
        .then(|| {
            let (pattern, range, search, invert) = OnMatchingLines::parse(query, &range)?;
            let start_idx = query.len() - pattern.len();
            query = query[start_idx..].trim();
            Some((range, search, invert))
        })
        .flatten();

    let mut action = if range.is_some() && query.is_empty() {
        Some(
            GoToLine {
                range: range.clone().unwrap(),
            }
            .boxed_clone(),
        )
    } else if query.starts_with('/') || query.starts_with('?') {
        Some(
            FindCommand {
                query: query[1..].to_string(),
                backwards: query.starts_with('?'),
            }
            .boxed_clone(),
        )
    } else if query.starts_with("se ") || query.starts_with("set ") {
        let (prefix, option) = query.split_once(' ').unwrap();
        let mut commands = VimOption::possible_commands(option);
        if !commands.is_empty() {
            let query = prefix.to_string() + " " + option;
            for command in &mut commands {
                command.positions = generate_positions(&command.string, &query);
            }
        }
        return Task::ready(CommandInterceptResult {
            results: commands,
            exclusive: false,
        });
    } else if query.starts_with('s') {
        let mut substitute = "substitute".chars().peekable();
        let mut query = query.chars().peekable();
        while substitute
            .peek()
            .is_some_and(|char| Some(char) == query.peek())
        {
            substitute.next();
            query.next();
        }
        if let Some(replacement) = Replacement::parse(query) {
            let range = range.clone().unwrap_or(CommandRange {
                start: Position::CurrentLine { offset: 0 },
                end: None,
            });
            Some(ReplaceCommand { replacement, range }.boxed_clone())
        } else {
            None
        }
    } else if query.contains('!') {
        ShellExec::parse(query, range.clone())
    } else if on_matching_lines.is_some() {
        commands(cx)
            .iter()
            .find_map(|command| command.parse(query, &None, cx))
    } else {
        None
    };

    if let Some((range, search, invert)) = on_matching_lines
        && let Some(ref inner) = action
    {
        action = Some(Box::new(OnMatchingLines {
            range,
            search,
            action: WrappedAction(inner.boxed_clone()),
            invert,
        }));
    };

    if let Some(action) = action {
        let string = input.to_string();
        let positions = generate_positions(&string, &(range_prefix + query));
        return Task::ready(CommandInterceptResult {
            results: vec![CommandInterceptItem {
                action,
                string,
                positions,
            }],
            exclusive: false,
        });
    }

    let Some((mut results, filenames)) =
        commands(cx).iter().enumerate().find_map(|(idx, command)| {
            let action = command.parse(query, &range, cx)?;
            let parsed_query = command.get_parsed_query(query.into())?;
            let display_string = ":".to_owned()
                + &range_prefix
                + command.prefix
                + command.suffix
                + if parsed_query.has_bang { "!" } else { "" };
            let space = if parsed_query.has_space { " " } else { "" };

            let string = format!("{}{}{}", &display_string, &space, &parsed_query.args);
            let positions = generate_positions(&string, &(range_prefix.clone() + query));

            let results = vec![CommandInterceptItem {
                action,
                string,
                positions,
            }];

            let no_args_positions =
                generate_positions(&display_string, &(range_prefix.clone() + query));

            // The following are valid autocomplete scenarios:
            // :w!filename.txt
            // :w filename.txt
            // :w[space]
            if !command.has_filename
                || (!has_trailing_space && !parsed_query.has_bang && parsed_query.args.is_empty())
            {
                return Some((results, None));
            }

            Some((
                results,
                Some((idx, parsed_query, display_string, no_args_positions)),
            ))
        })
    else {
        return Task::ready(CommandInterceptResult::default());
    };

    if let Some((cmd_idx, parsed_query, display_string, no_args_positions)) = filenames {
        let filenames = VimCommand::generate_filename_completions(&parsed_query, workspace, cx);
        cx.spawn(async move |cx| {
            let filenames = filenames.await;
            const MAX_RESULTS: usize = 100;
            let executor = cx.background_executor().clone();
            let mut candidates = Vec::with_capacity(filenames.len());

            for (idx, filename) in filenames.iter().enumerate() {
                candidates.push(fuzzy::StringMatchCandidate::new(idx, &filename));
            }
            let filenames = fuzzy::match_strings(
                &candidates,
                &parsed_query.args,
                false,
                true,
                MAX_RESULTS,
                &Default::default(),
                executor,
            )
            .await;

            for fuzzy::StringMatch {
                candidate_id: _,
                score: _,
                positions,
                string,
            } in filenames
            {
                let offset = display_string.len() + 1;
                let mut positions: Vec<_> = positions.iter().map(|&pos| pos + offset).collect();
                positions.splice(0..0, no_args_positions.clone());
                let string = format!("{display_string} {string}");
                let (range, query) = VimCommand::parse_range(&string[1..]);
                let action =
                    match cx.update(|cx| commands(cx).get(cmd_idx)?.parse(&query, &range, cx)) {
                        Some(action) => action,
                        _ => continue,
                    };
                results.push(CommandInterceptItem {
                    action,
                    string,
                    positions,
                });
            }
            CommandInterceptResult {
                results,
                exclusive: true,
            }
        })
    } else {
        Task::ready(CommandInterceptResult {
            results,
            exclusive: false,
        })
    }
}

fn generate_positions(string: &str, query: &str) -> Vec<usize> {
    let mut positions = Vec::new();
    let mut chars = query.chars();

    let Some(mut current) = chars.next() else {
        return positions;
    };

    for (i, c) in string.char_indices() {
        if c == current {
            positions.push(i);
            if let Some(c) = chars.next() {
                current = c;
            } else {
                break;
            }
        }
    }

    positions
}

/// Applies a command to all lines matching a pattern.
#[derive(Debug, PartialEq, Clone, Action)]
#[action(namespace = vim, no_json, no_register)]
pub(crate) struct OnMatchingLines {
    range: CommandRange,
    search: String,
    action: WrappedAction,
    invert: bool,
}
