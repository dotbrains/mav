use super::*;
use itertools::Either;
use smallvec::SmallVec;

pub fn styled_runs_for_code_label<'a>(
    label: &'a CodeLabel,
    syntax_theme: &'a theme::SyntaxTheme,
    local_player: &'a theme::PlayerColor,
) -> impl 'a + Iterator<Item = (Range<usize>, HighlightStyle)> {
    let fade_out = HighlightStyle {
        fade_out: Some(0.35),
        ..Default::default()
    };

    if label.runs.is_empty() {
        let desc_start = label.filter_range.end;
        let fade_run =
            (desc_start < label.text.len()).then(|| (desc_start..label.text.len(), fade_out));
        return Either::Left(fade_run.into_iter());
    }

    let mut prev_end = label.filter_range.end;
    Either::Right(
        label
            .runs
            .iter()
            .enumerate()
            .flat_map(move |(ix, (range, highlight_id))| {
                let style = if *highlight_id == language::HighlightId::TABSTOP_INSERT_ID {
                    HighlightStyle {
                        color: Some(local_player.cursor),
                        ..Default::default()
                    }
                } else if *highlight_id == language::HighlightId::TABSTOP_REPLACE_ID {
                    HighlightStyle {
                        background_color: Some(local_player.selection),
                        ..Default::default()
                    }
                } else if let Some(style) = syntax_theme.get(*highlight_id).cloned() {
                    style
                } else {
                    return Default::default();
                };

                let mut runs = SmallVec::<[(Range<usize>, HighlightStyle); 3]>::new();
                let muted_style = style.highlight(fade_out);
                if range.start >= label.filter_range.end {
                    if range.start > prev_end {
                        runs.push((prev_end..range.start, fade_out));
                    }
                    runs.push((range.clone(), muted_style));
                } else if range.end <= label.filter_range.end {
                    runs.push((range.clone(), style));
                } else {
                    runs.push((range.start..label.filter_range.end, style));
                    runs.push((label.filter_range.end..range.end, muted_style));
                }
                prev_end = cmp::max(prev_end, range.end);

                if ix + 1 == label.runs.len() && label.text.len() > prev_end {
                    runs.push((prev_end..label.text.len(), fade_out));
                }

                runs
            }),
    )
}
