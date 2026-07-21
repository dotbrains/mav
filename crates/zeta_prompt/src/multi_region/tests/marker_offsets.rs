use super::*;

#[test]
fn test_compute_marker_offsets_small_block() {
    let text = "aaa\nbbb\nccc\n";
    let offsets = compute_marker_offsets(text);
    assert_eq!(offsets, vec![0, text.len()]);
}

#[test]
fn test_compute_marker_offsets_blank_line_split() {
    let text = "aaa\nbbb\nccc\n\nddd\neee\nfff\n";
    let offsets = compute_marker_offsets(text);
    assert_eq!(offsets[0], 0);
    assert!(offsets.contains(&13), "offsets: {:?}", offsets);
    assert_eq!(*offsets.last().unwrap(), text.len());
}

#[test]
fn test_compute_marker_offsets_blank_line_split_overrides_pending_hard_cap_boundary() {
    let text = "\
class OCRDataframe(BaseModel):
    model_config = ConfigDict(arbitrary_types_allowed=True)

    df: pl.DataFrame

    def page(self, page_number: int = 0) -> \"OCRDataframe\":
        # Filter dataframe on specific page
        df_page = self.df.filter(pl.col(\"page\") == page_number)
        return OCRDataframe(df=df_page)

    def get_text_cell(
        self,
        cell: Cell,
        margin: int = 0,
        page_number: Optional[int] = None,
        min_confidence: int = 50,
    ) -> Optional[str]:
        \"\"\"
        Get text corresponding to cell
";
    let offsets = compute_marker_offsets(text);

    let def_start = text
        .find("    def get_text_cell(")
        .expect("def line exists");
    let self_start = text.find("        self,").expect("self line exists");

    assert!(
        offsets.contains(&def_start),
        "expected boundary at def line start ({def_start}), got {offsets:?}"
    );
    assert!(
        !offsets.contains(&self_start),
        "did not expect boundary at self line start ({self_start}), got {offsets:?}"
    );
}

#[test]
fn test_compute_marker_offsets_blank_line_split_skips_closer_line() {
    let text = "\
impl Plugin for AhoySchedulePlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            self.schedule,
            (
                AhoySystems::MoveCharacters,
                AhoySystems::ApplyForcesToDynamicRigidBodies,
            )
                .chain()
                .before(PhysicsSystems::First),
        );

    }
}

/// System set used by all systems of `bevy_ahoy`.
#[derive(SystemSet, Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum AhoySystems {
    MoveCharacters,
    ApplyForcesToDynamicRigidBodies,
}
";
    let offsets = compute_marker_offsets(text);

    let closer_start = text.find("    }\n").expect("closer line exists");
    let doc_start = text
        .find("/// System set used by all systems of `bevy_ahoy`.")
        .expect("doc line exists");

    assert!(
        !offsets.contains(&closer_start),
        "did not expect boundary at closer line start ({closer_start}), got {offsets:?}"
    );
    assert!(
        offsets.contains(&doc_start),
        "expected boundary at doc line start ({doc_start}), got {offsets:?}"
    );
}

#[test]
fn test_compute_marker_offsets_max_lines_split() {
    let text = "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n";
    let offsets = compute_marker_offsets(text);
    assert!(offsets.len() >= 3, "offsets: {:?}", offsets);
}

#[test]
fn test_compute_marker_offsets_hard_cap_nudges_past_closer_to_case_line() {
    let text = "a1\na2\na3\na4\na5\na6\na7\na8\n}\ncase 'x': {\nbody\n";
    let offsets = compute_marker_offsets(text);

    let expected = text.find("case 'x': {").expect("case line exists");
    assert!(
        offsets.contains(&expected),
        "expected nudged boundary at case line start ({expected}), got {offsets:?}"
    );
}

#[test]
fn test_compute_marker_offsets_hard_cap_nudge_respects_max_forward_lines() {
    let text = "a1\na2\na3\na4\na5\na6\na7\na8\n}\n}\n}\n}\n}\ncase 'x': {\nbody\n";
    let offsets = compute_marker_offsets(text);

    let case_start = text.find("case 'x': {").expect("case line exists");
    assert!(
        !offsets.contains(&case_start),
        "boundary should not nudge beyond max forward lines; offsets: {offsets:?}"
    );
}

#[test]
fn test_compute_marker_offsets_stay_sorted_when_hard_cap_boundary_nudges_forward() {
    let text = "\
aaaaaaaaaa = 1;
bbbbbbbbbb = 2;
cccccccccc = 3;
dddddddddd = 4;
eeeeeeeeee = 5;
ffffffffff = 6;
gggggggggg = 7;
hhhhhhhhhh = 8;
          };
        };

        grafanaDashboards = {
          cluster-overview.spec = {
            inherit instanceSelector;
            folderRef = \"infrastructure\";
            json = builtins.readFile ./grafana/dashboards/cluster-overview.json;
          };
        };
";
    let offsets = compute_marker_offsets(text);

    assert_eq!(offsets.first().copied(), Some(0), "offsets: {offsets:?}");
    assert_eq!(
        offsets.last().copied(),
        Some(text.len()),
        "offsets: {offsets:?}"
    );
    assert!(
        offsets.windows(2).all(|window| window[0] <= window[1]),
        "offsets must be sorted: {offsets:?}"
    );
}

#[test]
fn test_compute_marker_offsets_empty() {
    let offsets = compute_marker_offsets("");
    assert_eq!(offsets, vec![0, 0]);
}

#[test]
fn test_compute_v0327_editable_range_trims_to_marker_boundaries() {
    let text = (0..80).map(|_| "x\n").collect::<String>();
    let cursor_offset = text.find("x\nx\nx\nx\nx\n").expect("cursor anchor exists") + 40;

    let candidate_range = grow_v0327_candidate_range(&text, cursor_offset, 20);
    let editable_range = compute_v0327_editable_range(&text, cursor_offset, 20);
    let marker_offsets = compute_marker_offsets_v0318(&text[candidate_range.clone()]);
    let relative_start = editable_range.start - candidate_range.start;
    let relative_end = editable_range.end - candidate_range.start;

    assert!(
        marker_offsets.len() > 2,
        "expected interior markers: {marker_offsets:?}"
    );
    assert!(marker_offsets.contains(&relative_start));
    assert!(marker_offsets.contains(&relative_end));
    assert!(editable_range.start <= cursor_offset);
    assert!(editable_range.end >= cursor_offset);
    assert!(
        editable_range.start > candidate_range.start || editable_range.end < candidate_range.end,
        "expected at least one side to trim from {candidate_range:?} down to {editable_range:?}"
    );
}

#[test]
fn test_compute_marker_offsets_avoid_short_markdown_blocks() {
    let text = "\
# Spree Posts

This is a Posts extension for [Spree Commerce](https://spreecommerce.org), built with Ruby on Rails.

## Installation

1. Add this extension to your Gemfile with this line:

    ```ruby
    bundle add spree_posts
    ```

2. Run the install generator

    ```ruby
    bundle exec rails g spree_posts:install
    ```

3. Restart your server

  If your server was running, restart it so that it can find the assets properly.

## Developing

1. Create a dummy app

    ```bash
    bundle update
    bundle exec rake test_app
    ```

2. Add your new code
3. Run tests

    ```bash
    bundle exec rspec
    ```

When testing your applications integration with this extension you may use it's factories.
Simply add this require statement to your spec_helper:

```ruby
require 'spree_posts/factories'
```

## Releasing a new version

```shell
bundle exec gem bump -p -t
bundle exec gem release
```

For more options please see [gem-release README](https://github.com/svenfuchs/gem-release)

## Contributing

If you'd like to contribute, please take a look at the contributing guide.
";
    let offsets = compute_marker_offsets(text);

    assert_eq!(offsets.first().copied(), Some(0), "offsets: {offsets:?}");
    assert_eq!(
        offsets.last().copied(),
        Some(text.len()),
        "offsets: {offsets:?}"
    );

    for window in offsets.windows(2) {
        let block = &text[window[0]..window[1]];
        let line_count = block.lines().count();
        assert!(
            line_count >= V0316_MIN_BLOCK_LINES,
            "block too short: {line_count} lines in block {block:?} with offsets {offsets:?}"
        );
    }
}
