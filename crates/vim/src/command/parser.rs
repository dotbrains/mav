use super::*;

struct VimCommand {
    prefix: &'static str,
    suffix: &'static str,
    action: Option<Box<dyn Action>>,
    action_name: Option<&'static str>,
    bang_action: Option<Box<dyn Action>>,
    args: Option<
        Box<dyn Fn(Box<dyn Action>, String) -> Option<Box<dyn Action>> + Send + Sync + 'static>,
    >,
    /// Optional range Range to use if no range is specified.
    default_range: Option<CommandRange>,
    range: Option<
        Box<
            dyn Fn(Box<dyn Action>, &CommandRange) -> Option<Box<dyn Action>>
                + Send
                + Sync
                + 'static,
        >,
    >,
    has_count: bool,
    has_filename: bool,
}

struct ParsedQuery {
    args: String,
    has_bang: bool,
    has_space: bool,
}

impl VimCommand {
    fn new(pattern: (&'static str, &'static str), action: impl Action) -> Self {
        Self {
            prefix: pattern.0,
            suffix: pattern.1,
            action: Some(action.boxed_clone()),
            ..Default::default()
        }
    }

    // from_str is used for actions in other crates.
    fn str(pattern: (&'static str, &'static str), action_name: &'static str) -> Self {
        Self {
            prefix: pattern.0,
            suffix: pattern.1,
            action_name: Some(action_name),
            ..Default::default()
        }
    }

    fn bang(mut self, bang_action: impl Action) -> Self {
        self.bang_action = Some(bang_action.boxed_clone());
        self
    }

    /// Set argument handler. Trailing whitespace in arguments will be preserved.
    fn args(
        mut self,
        f: impl Fn(Box<dyn Action>, String) -> Option<Box<dyn Action>> + Send + Sync + 'static,
    ) -> Self {
        self.args = Some(Box::new(f));
        self
    }

    /// Set argument handler. Trailing whitespace in arguments will be trimmed.
    /// Supports filename autocompletion.
    fn filename(
        mut self,
        f: impl Fn(Box<dyn Action>, String) -> Option<Box<dyn Action>> + Send + Sync + 'static,
    ) -> Self {
        self.args = Some(Box::new(f));
        self.has_filename = true;
        self
    }

    fn range(
        mut self,
        f: impl Fn(Box<dyn Action>, &CommandRange) -> Option<Box<dyn Action>> + Send + Sync + 'static,
    ) -> Self {
        self.range = Some(Box::new(f));
        self
    }

    fn default_range(mut self, range: CommandRange) -> Self {
        self.default_range = Some(range);
        self
    }

    fn count(mut self) -> Self {
        self.has_count = true;
        self
    }

    fn generate_filename_completions(
        parsed_query: &ParsedQuery,
        workspace: WeakEntity<Workspace>,
        cx: &mut App,
    ) -> Task<Vec<String>> {
        let ParsedQuery {
            args,
            has_bang: _,
            has_space: _,
        } = parsed_query;
        let Some(workspace) = workspace.upgrade() else {
            return Task::ready(Vec::new());
        };

        let (task, args_path) = workspace.update(cx, |workspace, cx| {
            let prefix = workspace
                .project()
                .read(cx)
                .visible_worktrees(cx)
                .map(|worktree| worktree.read(cx).abs_path().to_path_buf())
                .next()
                .or_else(std::env::home_dir)
                .unwrap_or_else(|| PathBuf::from(""));

            let rel_path = match RelPath::new(Path::new(&args), PathStyle::local()) {
                Ok(path) => path.to_rel_path_buf(),
                Err(_) => {
                    return (Task::ready(Ok(Vec::new())), RelPathBuf::new());
                }
            };

            let rel_path = if args.ends_with(PathStyle::local().primary_separator()) {
                rel_path
            } else {
                rel_path
                    .parent()
                    .map(|rel_path| rel_path.to_rel_path_buf())
                    .unwrap_or(RelPathBuf::new())
            };

            let task = workspace.project().update(cx, |project, cx| {
                let path = prefix
                    .join(rel_path.as_std_path())
                    .to_string_lossy()
                    .to_string();
                project.list_directory(path, cx)
            });

            (task, rel_path)
        });

        cx.background_spawn(async move {
            let directories = task.await.unwrap_or_default();
            directories
                .iter()
                .map(|dir| {
                    let path = RelPath::new(dir.path.as_path(), PathStyle::local())
                        .map(|cow| cow.into_owned())
                        .unwrap_or(RelPathBuf::new());
                    let mut path_string = args_path
                        .join(&path)
                        .display(PathStyle::local())
                        .to_string();
                    if dir.is_dir {
                        path_string.push_str(PathStyle::local().primary_separator());
                    }
                    path_string
                })
                .collect()
        })
    }

    fn get_parsed_query(&self, query: String) -> Option<ParsedQuery> {
        let rest = query
            .strip_prefix(self.prefix)?
            .to_string()
            .chars()
            .zip_longest(self.suffix.to_string().chars())
            .skip_while(|e| e.clone().both().map(|(s, q)| s == q).unwrap_or(false))
            .filter_map(|e| e.left())
            .collect::<String>();
        let has_bang = rest.starts_with('!');
        let has_space = rest.starts_with("! ") || rest.starts_with(' ');
        let args = if has_bang {
            rest.strip_prefix('!')?.trim_start().to_string()
        } else if rest.is_empty() {
            "".into()
        } else {
            rest.strip_prefix(' ')?.trim_start().to_string()
        };
        Some(ParsedQuery {
            args,
            has_bang,
            has_space,
        })
    }

    fn parse(
        &self,
        query: &str,
        range: &Option<CommandRange>,
        cx: &App,
    ) -> Option<Box<dyn Action>> {
        let ParsedQuery {
            args,
            has_bang,
            has_space: _,
        } = self.get_parsed_query(query.to_string())?;
        let action = if has_bang && let Some(bang_action) = self.bang_action.as_ref() {
            bang_action.boxed_clone()
        } else if let Some(action) = self.action.as_ref() {
            action.boxed_clone()
        } else if let Some(action_name) = self.action_name {
            cx.build_action(action_name, None).log_err()?
        } else {
            return None;
        };

        // If the command does not accept args and we have args, we should do no
        // action.
        let action = if args.is_empty() {
            action
        } else if self.has_filename {
            self.args.as_ref()?(action, args.trim().into())?
        } else {
            self.args.as_ref()?(action, args)?
        };

        let range = range.as_ref().or(self.default_range.as_ref());
        if let Some(range) = range {
            self.range.as_ref().and_then(|f| f(action, range))
        } else {
            Some(action)
        }
    }

    // TODO: ranges with search queries
    fn parse_range(query: &str) -> (Option<CommandRange>, String) {
        let mut chars = query.chars().peekable();

        match chars.peek() {
            Some('%') => {
                chars.next();
                return (
                    Some(CommandRange {
                        start: Position::Line { row: 1, offset: 0 },
                        end: Some(Position::LastLine { offset: 0 }),
                    }),
                    chars.collect(),
                );
            }
            Some('*') => {
                chars.next();
                return (
                    Some(CommandRange {
                        start: Position::Mark {
                            name: '<',
                            offset: 0,
                        },
                        end: Some(Position::Mark {
                            name: '>',
                            offset: 0,
                        }),
                    }),
                    chars.collect(),
                );
            }
            _ => {}
        }

        let start = Self::parse_position(&mut chars);

        match chars.peek() {
            Some(',' | ';') => {
                chars.next();
                (
                    Some(CommandRange {
                        start: start.unwrap_or(Position::CurrentLine { offset: 0 }),
                        end: Self::parse_position(&mut chars),
                    }),
                    chars.collect(),
                )
            }
            _ => (
                start.map(|start| CommandRange { start, end: None }),
                chars.collect(),
            ),
        }
    }

    fn parse_position(chars: &mut Peekable<Chars>) -> Option<Position> {
        match chars.peek()? {
            '0'..='9' => {
                let row = Self::parse_u32(chars);
                Some(Position::Line {
                    row,
                    offset: Self::parse_offset(chars),
                })
            }
            '\'' => {
                chars.next();
                let name = chars.next()?;
                Some(Position::Mark {
                    name,
                    offset: Self::parse_offset(chars),
                })
            }
            '.' => {
                chars.next();
                Some(Position::CurrentLine {
                    offset: Self::parse_offset(chars),
                })
            }
            '+' | '-' => Some(Position::CurrentLine {
                offset: Self::parse_offset(chars),
            }),
            '$' => {
                chars.next();
                Some(Position::LastLine {
                    offset: Self::parse_offset(chars),
                })
            }
            _ => None,
        }
    }

    fn parse_offset(chars: &mut Peekable<Chars>) -> i32 {
        let mut res: i32 = 0;
        while matches!(chars.peek(), Some('+' | '-')) {
            let sign = if chars.next().unwrap() == '+' { 1 } else { -1 };
            let amount = if matches!(chars.peek(), Some('0'..='9')) {
                (Self::parse_u32(chars) as i32).saturating_mul(sign)
            } else {
                sign
            };
            res = res.saturating_add(amount)
        }
        res
    }

    fn parse_u32(chars: &mut Peekable<Chars>) -> u32 {
        let mut res: u32 = 0;
        while matches!(chars.peek(), Some('0'..='9')) {
            res = res
                .saturating_mul(10)
                .saturating_add(chars.next().unwrap() as u32 - '0' as u32);
        }
        res
    }
}

#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq)]
enum Position {
    Line { row: u32, offset: i32 },
    Mark { name: char, offset: i32 },
    LastLine { offset: i32 },
    CurrentLine { offset: i32 },
}

impl Position {
    fn buffer_row(
        &self,
        vim: &Vim,
        editor: &mut Editor,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<MultiBufferRow> {
        let snapshot = editor.snapshot(window, cx);
        let target = match self {
            Position::Line { row, offset } => {
                if let Some(anchor) = editor.active_buffer(cx).and_then(|buffer| {
                    editor.buffer().read(cx).buffer_point_to_anchor(
                        &buffer,
                        Point::new(row.saturating_sub(1), 0),
                        cx,
                    )
                }) {
                    anchor
                        .to_point(&snapshot.buffer_snapshot())
                        .row
                        .saturating_add_signed(*offset)
                } else {
                    row.saturating_add_signed(offset.saturating_sub(1))
                }
            }
            Position::Mark { name, offset } => {
                let Some(Mark::Local(anchors)) =
                    vim.get_mark(&name.to_string(), editor, window, cx)
                else {
                    anyhow::bail!("mark {name} not set");
                };
                let Some(mark) = anchors.last() else {
                    anyhow::bail!("mark {name} contains empty anchors");
                };
                mark.to_point(&snapshot.buffer_snapshot())
                    .row
                    .saturating_add_signed(*offset)
            }
            Position::LastLine { offset } => snapshot
                .buffer_snapshot()
                .max_row()
                .0
                .saturating_add_signed(*offset),
            Position::CurrentLine { offset } => editor
                .selections
                .newest_anchor()
                .head()
                .to_point(&snapshot.buffer_snapshot())
                .row
                .saturating_add_signed(*offset),
        };

        Ok(MultiBufferRow(target).min(snapshot.buffer_snapshot().max_row()))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CommandRange {
    start: Position,
    end: Option<Position>,
}

impl CommandRange {
    fn head(&self) -> &Position {
        self.end.as_ref().unwrap_or(&self.start)
    }

    /// Convert the `CommandRange` into a `Range<MultiBufferRow>`.
    pub(crate) fn buffer_range(
        &self,
        vim: &Vim,
        editor: &mut Editor,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<Range<MultiBufferRow>> {
        let start = self.start.buffer_row(vim, editor, window, cx)?;
        let end = if let Some(end) = self.end.as_ref() {
            end.buffer_row(vim, editor, window, cx)?
        } else {
            start
        };
        if end < start {
            anyhow::Ok(end..start)
        } else {
            anyhow::Ok(start..end)
        }
    }

    pub fn as_count(&self) -> Option<u32> {
        if let CommandRange {
            start: Position::Line { row, offset: 0 },
            end: None,
        } = &self
        {
            Some(*row)
        } else {
            None
        }
    }

    /// The `CommandRange` representing the entire buffer.
    fn buffer() -> Self {
        Self {
            start: Position::Line { row: 1, offset: 0 },
            end: Some(Position::LastLine { offset: 0 }),
        }
    }
}
