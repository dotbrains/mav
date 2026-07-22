use std::{collections::hash_map, sync::Arc, time::Duration};

use collections::{HashMap, HashSet};
use futures::future::join_all;
use gpui::{
    App, Context, FontStyle, FontWeight, HighlightStyle, StrikethroughStyle, Task, UnderlineStyle,
};
use itertools::Itertools;
use language::language_settings::LanguageSettings;
use project::{
    lsp_store::{
        BufferSemanticToken, BufferSemanticTokens, RefreshForServer, SemanticTokenStylizer,
        TokenType,
    },
    project_settings::ProjectSettings,
};
use settings::{
    SemanticTokenColorOverride, SemanticTokenFontStyle, SemanticTokenFontWeight, SemanticTokenRule,
    SemanticTokenRules, Settings as _,
};
use text::BufferId;
use theme::SyntaxTheme;
use ui::ActiveTheme as _;

use crate::{
    Editor,
    actions::ToggleSemanticHighlights,
    display_map::{HighlightStyleInterner, SemanticTokenHighlight},
};

pub(super) struct SemanticTokenState {
    rules: SemanticTokenRules,
    enabled: bool,
    update_task: Task<()>,
    fetched_for_buffers: HashMap<BufferId, clock::Global>,
}

impl SemanticTokenState {
    pub(super) fn new(cx: &App, enabled: bool) -> Self {
        Self {
            rules: ProjectSettings::get_global(cx)
                .global_lsp_settings
                .semantic_token_rules
                .clone(),
            enabled,
            update_task: Task::ready(()),
            fetched_for_buffers: HashMap::default(),
        }
    }

    pub(super) fn enabled(&self) -> bool {
        self.enabled
    }

    pub(super) fn toggle_enabled(&mut self) {
        self.enabled = !self.enabled;
    }

    #[cfg(test)]
    pub(super) fn take_update_task(&mut self) -> Task<()> {
        std::mem::replace(&mut self.update_task, Task::ready(()))
    }

    pub(super) fn invalidate_buffer(&mut self, buffer_id: &BufferId) {
        self.fetched_for_buffers.remove(buffer_id);
    }

    pub(super) fn update_rules(&mut self, new_rules: SemanticTokenRules) -> bool {
        if new_rules != self.rules {
            self.rules = new_rules;
            true
        } else {
            false
        }
    }
}

impl Editor {
    pub fn supports_semantic_tokens(&self, cx: &mut App) -> bool {
        let Some(provider) = self.semantics_provider.as_ref() else {
            return false;
        };

        let mut supports = false;
        self.buffer().update(cx, |this, cx| {
            this.for_each_buffer(&mut |buffer| {
                supports |= provider.supports_semantic_tokens(buffer, cx);
            });
        });

        supports
    }

    pub fn semantic_highlights_enabled(&self) -> bool {
        self.semantic_token_state.enabled()
    }

    pub fn toggle_semantic_highlights(
        &mut self,
        _: &ToggleSemanticHighlights,
        _window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        self.semantic_token_state.toggle_enabled();
        self.invalidate_semantic_tokens(None);
        self.refresh_semantic_tokens(None, None, cx);
    }

    pub(super) fn invalidate_semantic_tokens(&mut self, for_buffer: Option<BufferId>) {
        match for_buffer {
            Some(for_buffer) => self.semantic_token_state.invalidate_buffer(&for_buffer),
            None => self.semantic_token_state.fetched_for_buffers.clear(),
        }
    }

    pub(super) fn refresh_semantic_tokens(
        &mut self,
        buffer_id: Option<BufferId>,
        for_server: Option<RefreshForServer>,
        cx: &mut Context<Self>,
    ) {
        if !self.lsp_data_enabled() || !self.semantic_token_state.enabled() {
            self.invalidate_semantic_tokens(None);
            self.display_map.update(cx, |display_map, _| {
                match Arc::get_mut(&mut display_map.semantic_token_highlights) {
                    Some(highlights) => highlights.clear(),
                    None => display_map.semantic_token_highlights = Arc::new(Default::default()),
                };
            });
            self.semantic_token_state.update_task = Task::ready(());
            cx.notify();
            return;
        }

        let mut invalidate_semantic_highlights_for_buffers = HashSet::default();
        if for_server.is_some() {
            invalidate_semantic_highlights_for_buffers.extend(
                self.semantic_token_state
                    .fetched_for_buffers
                    .drain()
                    .map(|(buffer_id, _)| buffer_id),
            );
        }

        let Some((sema, project)) = self
            .semantics_provider
            .clone()
            .zip(self.project.as_ref().map(|p| p.downgrade()))
        else {
            return;
        };

        let buffers_to_query = self
            .visible_buffers(cx)
            .into_iter()
            .filter(|buffer| self.is_lsp_relevant(buffer.read(cx).file(), cx))
            .chain(buffer_id.and_then(|buffer_id| self.buffer.read(cx).buffer(buffer_id)))
            .filter_map(|editor_buffer| {
                let editor_buffer_id = editor_buffer.read(cx).remote_id();
                if self.registered_buffers.contains_key(&editor_buffer_id)
                    && LanguageSettings::for_buffer(editor_buffer.read(cx), cx)
                        .semantic_tokens
                        .enabled()
                {
                    Some((editor_buffer_id, editor_buffer))
                } else {
                    None
                }
            })
            .collect::<HashMap<_, _>>();

        for buffer_with_disabled_tokens in self
            .display_map
            .read(cx)
            .semantic_token_highlights
            .keys()
            .copied()
            .filter(|buffer_id| !buffers_to_query.contains_key(buffer_id))
            .filter(|buffer_id| {
                !self
                    .buffer
                    .read(cx)
                    .buffer(*buffer_id)
                    .is_some_and(|buffer| {
                        let buffer = buffer.read(cx);
                        LanguageSettings::for_buffer(&buffer, cx)
                            .semantic_tokens
                            .enabled()
                    })
            })
            .collect::<Vec<_>>()
        {
            self.semantic_token_state
                .invalidate_buffer(&buffer_with_disabled_tokens);
            self.display_map.update(cx, |display_map, _| {
                display_map.invalidate_semantic_highlights(buffer_with_disabled_tokens);
            });
        }

        self.semantic_token_state.update_task = cx.spawn(async move |editor, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(50))
                .await;
            let Some(all_semantic_tokens_task) = editor
                .update(cx, |editor, cx| {
                    buffers_to_query
                        .into_iter()
                        .filter_map(|(buffer_id, buffer)| {
                            let known_version = editor
                                .semantic_token_state
                                .fetched_for_buffers
                                .get(&buffer_id);
                            let query_version = buffer.read(cx).version();
                            if known_version.is_some_and(|known_version| {
                                !query_version.changed_since(known_version)
                            }) {
                                None
                            } else {
                                sema.semantic_tokens(buffer, for_server, cx).map(
                                    |task| async move { (buffer_id, query_version, task.await) },
                                )
                            }
                        })
                        .collect::<Vec<_>>()
                })
                .ok()
            else {
                return;
            };

            let all_semantic_tokens = join_all(all_semantic_tokens_task).await;
            editor
                .update(cx, |editor, cx| {
                    editor.display_map.update(cx, |display_map, _| {
                        for buffer_id in invalidate_semantic_highlights_for_buffers {
                            display_map.invalidate_semantic_highlights(buffer_id);
                            editor.semantic_token_state.invalidate_buffer(&buffer_id);
                        }
                    });

                    if all_semantic_tokens.is_empty() {
                        return;
                    }
                    let multi_buffer_snapshot = editor.buffer().read(cx).snapshot(cx);

                    for (buffer_id, query_version, tokens) in all_semantic_tokens {
                        let tokens = match tokens {
                            Ok(BufferSemanticTokens {
                                tokens: Some(tokens),
                            }) => tokens,
                            Ok(BufferSemanticTokens { tokens: None }) => {
                                editor.display_map.update(cx, |display_map, _| {
                                    display_map.invalidate_semantic_highlights(buffer_id);
                                });
                                continue;
                            }
                            Err(e) => {
                                log::error!(
                                    "Failed to fetch semantic tokens for buffer \
                                    {buffer_id:?}: {e:#}"
                                );
                                continue;
                            }
                        };

                        match editor
                            .semantic_token_state
                            .fetched_for_buffers
                            .entry(buffer_id)
                        {
                            hash_map::Entry::Occupied(mut o) => {
                                if query_version.changed_since(o.get()) {
                                    o.insert(query_version);
                                } else {
                                    continue;
                                }
                            }
                            hash_map::Entry::Vacant(v) => {
                                v.insert(query_version);
                            }
                        }

                        let language_name = editor
                            .buffer()
                            .read(cx)
                            .buffer(buffer_id)
                            .and_then(|buf| buf.read(cx).language().map(|l| l.name()));

                        let Some(project) = project.upgrade() else {
                            return;
                        };
                        editor.display_map.update(cx, |display_map, cx| {
                            project.read(cx).lsp_store().update(cx, |lsp_store, cx| {
                                let mut token_highlights = Vec::new();
                                let mut interner = HighlightStyleInterner::default();
                                for (server_id, server_tokens) in tokens {
                                    let Some(stylizer) = lsp_store.get_or_create_token_stylizer(
                                        server_id,
                                        language_name.as_ref(),
                                        cx,
                                    ) else {
                                        continue;
                                    };
                                    let theme = cx.theme().syntax();
                                    token_highlights.reserve(2 * server_tokens.len());
                                    token_highlights.extend(buffer_into_editor_highlights(
                                        &server_tokens,
                                        stylizer,
                                        &multi_buffer_snapshot,
                                        &mut interner,
                                        theme,
                                    ));
                                }

                                token_highlights.sort_by(|a, b| {
                                    a.range.start.cmp(&b.range.start, &multi_buffer_snapshot)
                                });
                                Arc::make_mut(&mut display_map.semantic_token_highlights).insert(
                                    buffer_id,
                                    (Arc::from(token_highlights), Arc::new(interner)),
                                );
                            });
                        });
                    }

                    cx.notify();
                })
                .ok();
        });
    }
}

fn buffer_into_editor_highlights<'a, 'b>(
    buffer_tokens: &'a [BufferSemanticToken],
    stylizer: &'a SemanticTokenStylizer,
    multi_buffer_snapshot: &'a multi_buffer::MultiBufferSnapshot,
    interner: &'b mut HighlightStyleInterner,
    theme: &'a SyntaxTheme,
) -> impl Iterator<Item = SemanticTokenHighlight> + use<'a, 'b> {
    multi_buffer_snapshot
        .text_anchors_to_visible_anchors(
            buffer_tokens
                .iter()
                .flat_map(|token| [token.range.start, token.range.end]),
        )
        .into_iter()
        .tuples::<(_, _)>()
        .zip(buffer_tokens)
        .filter_map(|((multi_buffer_start, multi_buffer_end), token)| {
            let range = multi_buffer_start?..multi_buffer_end?;
            let style = convert_token(stylizer, theme, token.token_type, token.token_modifiers)?;
            let style = interner.intern(style);
            Some(SemanticTokenHighlight {
                range,
                style,
                token_type: token.token_type,
                token_modifiers: token.token_modifiers,
                server_id: stylizer.server_id(),
            })
        })
}

fn convert_token(
    stylizer: &SemanticTokenStylizer,
    theme: &SyntaxTheme,
    token_type: TokenType,
    modifiers: u32,
) -> Option<HighlightStyle> {
    let rules = stylizer.rules_for_token(token_type)?;
    let filter = |rule: &&SemanticTokenRule| {
        rule.token_modifiers
            .iter()
            .all(|m| stylizer.has_modifier(modifiers, m))
    };
    let last = rules.last()?;
    if last.no_style_defined() && filter(&last) {
        return None;
    }

    let mut highlight = HighlightStyle::default();

    for rule in rules.into_iter().filter(filter) {
        let style = rule
            .style
            .iter()
            .find_map(|style| theme.style_for_name(style));

        macro_rules! overwrite {
            (
                highlight.$highlight_field:ident,
                SemanticTokenRule::$rule_field:ident,
                $transform:expr $(,)?
            ) => {
                highlight.$highlight_field = rule
                    .$rule_field
                    .map($transform)
                    .or_else(|| style.as_ref().and_then(|s| s.$highlight_field))
                    .or(highlight.$highlight_field)
            };
        }

        overwrite!(
            highlight.color,
            SemanticTokenRule::foreground_color,
            Into::into,
        );

        overwrite!(
            highlight.background_color,
            SemanticTokenRule::background_color,
            Into::into,
        );

        overwrite!(
            highlight.font_weight,
            SemanticTokenRule::font_weight,
            |w| match w {
                SemanticTokenFontWeight::Normal => FontWeight::NORMAL,
                SemanticTokenFontWeight::Bold => FontWeight::BOLD,
            },
        );

        overwrite!(
            highlight.font_style,
            SemanticTokenRule::font_style,
            |s| match s {
                SemanticTokenFontStyle::Normal => FontStyle::Normal,
                SemanticTokenFontStyle::Italic => FontStyle::Italic,
            },
        );

        overwrite!(highlight.underline, SemanticTokenRule::underline, |u| {
            UnderlineStyle {
                thickness: 1.0.into(),
                color: match u {
                    SemanticTokenColorOverride::InheritForeground(true) => highlight.color,
                    SemanticTokenColorOverride::InheritForeground(false) => None,
                    SemanticTokenColorOverride::Replace(c) => Some(c.into()),
                },
                ..UnderlineStyle::default()
            }
        });

        overwrite!(
            highlight.strikethrough,
            SemanticTokenRule::strikethrough,
            |s| StrikethroughStyle {
                thickness: 1.0.into(),
                color: match s {
                    SemanticTokenColorOverride::InheritForeground(true) => highlight.color,
                    SemanticTokenColorOverride::InheritForeground(false) => None,
                    SemanticTokenColorOverride::Replace(c) => Some(c.into()),
                },
            },
        );
    }
    Some(highlight)
}

#[cfg(test)]
#[cfg(test)]
mod tests;
