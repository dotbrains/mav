use super::*;

impl<T: PromptCompletionProviderDelegate> PromptCompletionProvider<T> {
    fn completion_for_diagnostics(
        source_range: Range<Anchor>,
        source: Arc<T>,
        editor: WeakEntity<Editor>,
        mention_set: WeakEntity<MentionSet>,
        workspace: Entity<Workspace>,
        cx: &mut App,
    ) -> Vec<Completion> {
        let summary = workspace
            .read(cx)
            .project()
            .read(cx)
            .diagnostic_summary(false, cx);
        if summary.error_count == 0 && summary.warning_count == 0 {
            return Vec::new();
        }
        let icon_path = MentionUri::Diagnostics {
            include_errors: true,
            include_warnings: false,
        }
        .icon_path(cx);

        let mut completions = Vec::new();

        let cases = [
            (summary.error_count > 0, true, false),
            (summary.warning_count > 0, false, true),
            (
                summary.error_count > 0 && summary.warning_count > 0,
                true,
                true,
            ),
        ];

        for (condition, include_errors, include_warnings) in cases {
            if condition {
                completions.push(Self::build_diagnostics_completion(
                    diagnostics_submenu_label(summary, include_errors, include_warnings),
                    source_range.clone(),
                    source.clone(),
                    editor.clone(),
                    mention_set.clone(),
                    workspace.clone(),
                    icon_path.clone(),
                    include_errors,
                    include_warnings,
                    summary,
                ));
            }
        }

        completions
    }

    fn build_diagnostics_completion(
        menu_label: String,
        source_range: Range<Anchor>,
        source: Arc<T>,
        editor: WeakEntity<Editor>,
        mention_set: WeakEntity<MentionSet>,
        workspace: Entity<Workspace>,
        icon_path: SharedString,
        include_errors: bool,
        include_warnings: bool,
        summary: DiagnosticSummary,
    ) -> Completion {
        let uri = MentionUri::Diagnostics {
            include_errors,
            include_warnings,
        };
        let crease_text = diagnostics_crease_label(summary, include_errors, include_warnings);
        let display_text = format!("@{}", crease_text);
        let new_text = format!("[{}]({}) ", display_text, uri.to_uri());
        let new_text_len = new_text.len();
        Completion {
            replace_range: source_range.clone(),
            new_text,
            label: CodeLabel::plain(menu_label, None),
            documentation: None,
            source: project::CompletionSource::Custom,
            icon_path: Some(icon_path),
            icon_color: None,
            match_start: None,
            snippet_deduplication_key: None,
            insert_text_mode: None,
            confirm: Some(confirm_completion_callback(
                crease_text,
                source_range.start,
                new_text_len - 1,
                uri,
                source,
                editor,
                mention_set,
                workspace,
            )),
            group: None,
        }
    }

    fn build_branch_diff_completion(
        base_ref: SharedString,
        source_range: Range<Anchor>,
        source: Arc<T>,
        editor: WeakEntity<Editor>,
        mention_set: WeakEntity<MentionSet>,
        workspace: Entity<Workspace>,
        cx: &mut App,
    ) -> Completion {
        let uri = MentionUri::GitDiff {
            base_ref: base_ref.to_string(),
        };
        let crease_text: SharedString = format!("Branch Diff (vs {})", base_ref).into();
        let display_text = format!("@{}", crease_text);
        let new_text = format!("[{}]({}) ", display_text, uri.to_uri());
        let new_text_len = new_text.len();
        let icon_path = uri.icon_path(cx);

        Completion {
            replace_range: source_range.clone(),
            new_text,
            label: CodeLabel::plain(crease_text.to_string(), None),
            documentation: None,
            source: project::CompletionSource::Custom,
            icon_path: Some(icon_path),
            icon_color: None,
            match_start: None,
            snippet_deduplication_key: None,
            insert_text_mode: None,
            confirm: Some(confirm_completion_callback(
                crease_text,
                source_range.start,
                new_text_len - 1,
                uri,
                source,
                editor,
                mention_set,
                workspace,
            )),
            group: None,
        }
    }
}
