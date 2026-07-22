use super::*;

#[derive(Debug, PartialEq)]
pub(super) enum HyperlinkKind {
    FileIri,
    Iri,
    Path,
}

struct ExpectedHyperlink {
    hovered_grid_point: AlacPoint,
    hovered_char: char,
    hyperlink_kind: HyperlinkKind,
    iri_or_path: String,
    row: Option<u32>,
    column: Option<u32>,
    hyperlink_match: RangeInclusive<AlacPoint>,
}

/// Converts to Windows style paths on Windows, like path!(), but at runtime for improved test
/// readability.
fn build_term_from_test_lines<'a>(
    hyperlink_kind: HyperlinkKind,
    term_size: TermSize,
    test_lines: impl Iterator<Item = &'a str>,
) -> (Term<VoidListener>, ExpectedHyperlink) {
    #[derive(Default, Eq, PartialEq)]
    enum HoveredState {
        #[default]
        HoveredScan,
        HoveredNextChar,
        Done,
    }

    #[derive(Default, Eq, PartialEq)]
    enum MatchState {
        #[default]
        MatchScan,
        MatchNextChar,
        Match(AlacPoint),
        Done,
    }

    #[derive(Default, Eq, PartialEq)]
    enum CapturesState {
        #[default]
        PathScan,
        PathNextChar,
        Path(AlacPoint),
        RowScan,
        Row(String),
        ColumnScan,
        Column(String),
        Done,
    }

    fn prev_input_point_from_term(term: &Term<VoidListener>) -> AlacPoint {
        let grid = term.grid();
        let cursor = &grid.cursor;
        let mut point = cursor.point;

        if !cursor.input_needs_wrap {
            point = point.sub(term, Boundary::Grid, 1);
        }

        if grid.index(point).flags.contains(Flags::WIDE_CHAR_SPACER) {
            point.column -= 1;
        }

        point
    }

    fn end_point_from_prev_input_point(
        term: &Term<VoidListener>,
        prev_input_point: AlacPoint,
    ) -> AlacPoint {
        if term
            .grid()
            .index(prev_input_point)
            .flags
            .contains(Flags::WIDE_CHAR)
        {
            prev_input_point.add(term, Boundary::Grid, 1)
        } else {
            prev_input_point
        }
    }

    fn process_input(term: &mut Term<VoidListener>, c: char) {
        match c {
            '\t' => term.put_tab(1),
            c @ _ => term.input(c),
        }
    }

    let mut hovered_grid_point: Option<AlacPoint> = None;
    let mut hyperlink_match = AlacPoint::default()..=AlacPoint::default();
    let mut iri_or_path = String::default();
    let mut row = None;
    let mut column = None;
    let mut prev_input_point = AlacPoint::default();
    let mut hovered_state = HoveredState::default();
    let mut match_state = MatchState::default();
    let mut captures_state = CapturesState::default();
    let mut term = Term::new(Config::default(), &term_size, VoidListener);

    for text in test_lines {
        let chars: Box<dyn Iterator<Item = char>> =
            if cfg!(windows) && hyperlink_kind == HyperlinkKind::Path {
                Box::new(text.chars().map(|c| if c == '/' { '\\' } else { c })) as _
            } else {
                Box::new(text.chars()) as _
            };
        let mut chars = chars.peekable();
        while let Some(c) = chars.next() {
            match c {
                '👉' => {
                    hovered_state = HoveredState::HoveredNextChar;
                }
                '👈' => {
                    hovered_grid_point = Some(prev_input_point.add(&term, Boundary::Grid, 1));
                }
                '«' | '»' => {
                    captures_state = match captures_state {
                        CapturesState::PathScan => CapturesState::PathNextChar,
                        CapturesState::PathNextChar => {
                            panic!("Should have been handled by char input")
                        }
                        CapturesState::Path(start_point) => {
                            iri_or_path = term.bounds_to_string(
                                start_point,
                                end_point_from_prev_input_point(&term, prev_input_point),
                            );
                            CapturesState::RowScan
                        }
                        CapturesState::RowScan => CapturesState::Row(String::new()),
                        CapturesState::Row(number) => {
                            row = Some(number.parse::<u32>().unwrap());
                            CapturesState::ColumnScan
                        }
                        CapturesState::ColumnScan => CapturesState::Column(String::new()),
                        CapturesState::Column(number) => {
                            column = Some(number.parse::<u32>().unwrap());
                            CapturesState::Done
                        }
                        CapturesState::Done => {
                            panic!("Extra '«', '»'")
                        }
                    }
                }
                '‹' | '›' => {
                    match_state = match match_state {
                        MatchState::MatchScan => MatchState::MatchNextChar,
                        MatchState::MatchNextChar => {
                            panic!("Should have been handled by char input")
                        }
                        MatchState::Match(start_point) => {
                            hyperlink_match = start_point
                                ..=end_point_from_prev_input_point(&term, prev_input_point);
                            MatchState::Done
                        }
                        MatchState::Done => {
                            panic!("Extra '‹', '›'")
                        }
                    }
                }
                _ => {
                    if let CapturesState::Row(number) | CapturesState::Column(number) =
                        &mut captures_state
                    {
                        number.push(c)
                    }

                    let is_windows_abs_path_start = captures_state == CapturesState::PathNextChar
                        && cfg!(windows)
                        && hyperlink_kind == HyperlinkKind::Path
                        && c == '\\'
                        && chars.peek().is_some_and(|c| *c != '\\');

                    if is_windows_abs_path_start {
                        // Convert Unix abs path start into Windows abs path start so that the
                        // same test can be used for both OSes.
                        term.input('C');
                        prev_input_point = prev_input_point_from_term(&term);
                        term.input(':');
                        process_input(&mut term, c);
                    } else {
                        process_input(&mut term, c);
                        prev_input_point = prev_input_point_from_term(&term);
                    }

                    if hovered_state == HoveredState::HoveredNextChar {
                        hovered_grid_point = Some(prev_input_point);
                        hovered_state = HoveredState::Done;
                    }
                    if captures_state == CapturesState::PathNextChar {
                        captures_state = CapturesState::Path(prev_input_point);
                    }
                    if match_state == MatchState::MatchNextChar {
                        match_state = MatchState::Match(prev_input_point);
                    }
                }
            }
        }
        term.move_down_and_cr(1);
    }

    if hyperlink_kind == HyperlinkKind::FileIri {
        let Ok(url) = Url::parse(&iri_or_path) else {
            panic!("Failed to parse file IRI `{iri_or_path}`");
        };
        let Ok(path) = url.to_file_path() else {
            panic!("Failed to interpret file IRI `{iri_or_path}` as a path");
        };
        iri_or_path = path.to_string_lossy().into_owned();
    }

    let hovered_grid_point = hovered_grid_point.expect("Missing hovered point (👉 or 👈)");
    let hovered_char = term.grid().index(hovered_grid_point).c;
    (
        term,
        ExpectedHyperlink {
            hovered_grid_point,
            hovered_char,
            hyperlink_kind,
            iri_or_path,
            row,
            column,
            hyperlink_match,
        },
    )
}

pub(super) fn line_cells_count(line: &str) -> usize {
    // This avoids taking a dependency on the unicode-width crate
    fn width(c: char) -> usize {
        match c {
            // Fullwidth unicode characters used in tests
            '例' | '🏃' | '🦀' | '🔥' => 2,
            '\t' => 8, // it's really 0-8, use the max always
            _ => 1,
        }
    }
    const CONTROL_CHARS: &str = "‹«👉👈»›";
    line.chars()
        .filter(|c| !CONTROL_CHARS.contains(*c))
        .map(width)
        .sum::<usize>()
}

struct CheckHyperlinkMatch<'a> {
    term: &'a Term<VoidListener>,
    expected_hyperlink: &'a ExpectedHyperlink,
    source_location: &'a str,
}

impl<'a> CheckHyperlinkMatch<'a> {
    fn new(
        term: &'a Term<VoidListener>,
        expected_hyperlink: &'a ExpectedHyperlink,
        source_location: &'a str,
    ) -> Self {
        Self {
            term,
            expected_hyperlink,
            source_location,
        }
    }

    fn check_path_with_position_and_match(
        &self,
        path_with_position: PathWithPosition,
        hyperlink_match: &Match,
    ) {
        let format_path_with_position_and_match =
            |path_with_position: &PathWithPosition, hyperlink_match: &Match| {
                let mut result = format!("Path = «{}»", &path_with_position.path.to_string_lossy());
                if let Some(row) = path_with_position.row {
                    result += &format!(", line = {row}");
                    if let Some(column) = path_with_position.column {
                        result += &format!(", column = {column}");
                    }
                }

                result += &format!(
                    ", at grid cells {}",
                    Self::format_hyperlink_match(hyperlink_match)
                );
                result
            };

        assert_ne!(
            self.expected_hyperlink.hyperlink_kind,
            HyperlinkKind::Iri,
            "\n    at {}\nExpected a path, but was a iri:\n{}",
            self.source_location,
            self.format_renderable_content()
        );

        assert_eq!(
            format_path_with_position_and_match(
                &PathWithPosition {
                    path: PathBuf::from(self.expected_hyperlink.iri_or_path.clone()),
                    row: self.expected_hyperlink.row,
                    column: self.expected_hyperlink.column
                },
                &self.expected_hyperlink.hyperlink_match
            ),
            format_path_with_position_and_match(&path_with_position, hyperlink_match),
            "\n    at {}:\n{}",
            self.source_location,
            self.format_renderable_content()
        );
    }

    fn check_iri_and_match(&self, iri: String, hyperlink_match: &Match) {
        let format_iri_and_match = |iri: &String, hyperlink_match: &Match| {
            format!(
                "Url = «{iri}», at grid cells {}",
                Self::format_hyperlink_match(hyperlink_match)
            )
        };

        assert_eq!(
            self.expected_hyperlink.hyperlink_kind,
            HyperlinkKind::Iri,
            "\n    at {}\nExpected a iri, but was a path:\n{}",
            self.source_location,
            self.format_renderable_content()
        );

        assert_eq!(
            format_iri_and_match(
                &self.expected_hyperlink.iri_or_path,
                &self.expected_hyperlink.hyperlink_match
            ),
            format_iri_and_match(&iri, hyperlink_match),
            "\n    at {}:\n{}",
            self.source_location,
            self.format_renderable_content()
        );
    }

    fn format_hyperlink_match(hyperlink_match: &Match) -> String {
        format!(
            "({}, {})..=({}, {})",
            hyperlink_match.start().line.0,
            hyperlink_match.start().column.0,
            hyperlink_match.end().line.0,
            hyperlink_match.end().column.0
        )
    }

    fn format_renderable_content(&self) -> String {
        let mut result = format!("\nHovered on '{}'\n", self.expected_hyperlink.hovered_char);

        let mut first_header_row = String::new();
        let mut second_header_row = String::new();
        let mut marker_header_row = String::new();
        for index in 0..self.term.columns() {
            let remainder = index % 10;
            if index > 0 && remainder == 0 {
                first_header_row.push_str(&format!("{:>10}", (index / 10)));
            }
            second_header_row += &remainder.to_string();
            if index == self.expected_hyperlink.hovered_grid_point.column.0 {
                marker_header_row.push('↓');
            } else {
                marker_header_row.push(' ');
            }
        }

        let remainder = (self.term.columns() - 1) % 10;
        if remainder != 0 {
            first_header_row.push_str(&" ".repeat(remainder));
        }

        result += &format!("\n      [ {}]\n", first_header_row);
        result += &format!("      [{}]\n", second_header_row);
        result += &format!("       {}", marker_header_row);

        for cell in self
            .term
            .renderable_content()
            .display_iter
            .filter(|cell| !cell.flags.intersects(WIDE_CHAR_SPACERS))
        {
            if cell.point.column.0 == 0 {
                let prefix = if cell.point.line == self.expected_hyperlink.hovered_grid_point.line {
                    '→'
                } else {
                    ' '
                };
                result += &format!("\n{prefix}[{:>3}] ", cell.point.line.to_string());
            }

            match cell.c {
                '\t' => result.push(' '),
                c @ _ => result.push(c),
            }
        }

        result
    }
}

pub(super) fn test_hyperlink<'a>(
    columns: usize,
    total_cells: usize,
    test_lines: impl Iterator<Item = &'a str>,
    hyperlink_kind: HyperlinkKind,
    source_location: &str,
) {
    const CARGO_DIR_REGEX: &str =
        r#"\s+(Compiling|Checking|Documenting) [^(]+\((?<link>(?<path>.+))\)"#;
    const RUST_DIAGNOSTIC_REGEX: &str = r#"\s+(-->|:::|at) (?<link>(?<path>.+?))(:$|$)"#;
    const ISSUE_12338_REGEX: &str = r#"[0-9]{2}-[0-9]{2} [0-9]{2}:[0-9]{2} (?<link>(?<path>.+))"#;
    const MULTIPLE_SAME_LINE_REGEX: &str =
        r#"(?<link>(?<path>🦀 multiple_same_line 🦀) 🚣(?<line>[0-9]+) 🏛(?<column>[0-9]+)):"#;
    const PATH_HYPERLINK_TIMEOUT_MS: u64 = 1000;

    thread_local! {
        static TEST_REGEX_SEARCHES: RefCell<RegexSearches> =
            RefCell::new({
                let default_settings_content: Rc<SettingsContent> =
                    settings::parse_json_with_comments(&settings::default_settings()).unwrap();
                let default_terminal_settings = TerminalSettings::from_settings(&default_settings_content);

                RegexSearches::new([
                    RUST_DIAGNOSTIC_REGEX,
                    CARGO_DIR_REGEX,
                    ISSUE_12338_REGEX,
                    MULTIPLE_SAME_LINE_REGEX,
                ]
                    .into_iter()
                    .chain(default_terminal_settings.path_hyperlink_regexes
                        .iter()
                        .map(AsRef::as_ref)),
                PATH_HYPERLINK_TIMEOUT_MS)
            });
    }

    let term_size = TermSize::new(columns, total_cells / columns + 2);
    let (term, expected_hyperlink) =
        build_term_from_test_lines(hyperlink_kind, term_size, test_lines);
    let hyperlink_found = TEST_REGEX_SEARCHES.with(|regex_searches| {
        find_from_grid_point(
            &term,
            expected_hyperlink.hovered_grid_point,
            &mut regex_searches.borrow_mut(),
            PathStyle::local(),
        )
    });
    let check_hyperlink_match =
        CheckHyperlinkMatch::new(&term, &expected_hyperlink, source_location);
    match hyperlink_found {
        Some(hyperlink) if !hyperlink.is_url => {
            let hyperlink_match = hyperlink.range.to_alacritty();
            check_hyperlink_match.check_path_with_position_and_match(
                PathWithPosition::parse_str(&hyperlink.text),
                &hyperlink_match,
            );
        }
        Some(hyperlink) => {
            let hyperlink_match = hyperlink.range.to_alacritty();
            check_hyperlink_match.check_iri_and_match(hyperlink.text, &hyperlink_match);
        }
        None => {
            if expected_hyperlink.hyperlink_match.start()
                != expected_hyperlink.hyperlink_match.end()
            {
                assert!(
                    false,
                    "No hyperlink found\n     at {source_location}:\n{}",
                    check_hyperlink_match.format_renderable_content()
                )
            }
        }
    }
}
