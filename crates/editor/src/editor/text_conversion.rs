use super::*;
use convert_case::{Case, Casing};

impl Editor {
    pub fn convert_to_upper_case(
        &mut self,
        _: &ConvertToUpperCase,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.manipulate_text(window, cx, |text| text.to_uppercase())
    }

    pub fn convert_to_lower_case(
        &mut self,
        _: &ConvertToLowerCase,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.manipulate_text(window, cx, |text| text.to_lowercase())
    }

    pub fn convert_to_title_case(
        &mut self,
        _: &ConvertToTitleCase,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.manipulate_text(window, cx, |text| {
            Self::convert_text_case(text, Case::Title)
        })
    }

    pub fn convert_to_snake_case(
        &mut self,
        _: &ConvertToSnakeCase,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.manipulate_text(window, cx, |text| {
            Self::convert_text_case(text, Case::Snake)
        })
    }

    pub fn convert_to_kebab_case(
        &mut self,
        _: &ConvertToKebabCase,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.manipulate_text(window, cx, |text| {
            Self::convert_text_case(text, Case::Kebab)
        })
    }

    pub fn convert_to_upper_camel_case(
        &mut self,
        _: &ConvertToUpperCamelCase,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.manipulate_text(window, cx, |text| {
            Self::convert_text_case(text, Case::UpperCamel)
        })
    }

    pub fn convert_to_lower_camel_case(
        &mut self,
        _: &ConvertToLowerCamelCase,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.manipulate_text(window, cx, |text| {
            Self::convert_text_case(text, Case::Camel)
        })
    }

    pub fn convert_to_opposite_case(
        &mut self,
        _: &ConvertToOppositeCase,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.manipulate_text(window, cx, |text| {
            text.chars()
                .fold(String::with_capacity(text.len()), |mut t, c| {
                    if c.is_uppercase() {
                        t.extend(c.to_lowercase());
                    } else {
                        t.extend(c.to_uppercase());
                    }
                    t
                })
        })
    }

    pub fn convert_to_sentence_case(
        &mut self,
        _: &ConvertToSentenceCase,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.manipulate_text(window, cx, |text| {
            Self::convert_text_case(text, Case::Sentence)
        })
    }

    pub fn toggle_case(&mut self, _: &ToggleCase, window: &mut Window, cx: &mut Context<Self>) {
        self.manipulate_text(window, cx, |text| {
            let has_upper_case_characters = text.chars().any(|c| c.is_uppercase());
            if has_upper_case_characters {
                text.to_lowercase()
            } else {
                text.to_uppercase()
            }
        })
    }

    pub fn convert_to_rot13(
        &mut self,
        _: &ConvertToRot13,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.manipulate_text(window, cx, |text| {
            text.chars()
                .map(|c| match c {
                    'A'..='M' | 'a'..='m' => ((c as u8) + 13) as char,
                    'N'..='Z' | 'n'..='z' => ((c as u8) - 13) as char,
                    _ => c,
                })
                .collect()
        })
    }

    fn convert_text_case(text: &str, case: Case) -> String {
        text.lines()
            .map(|line| {
                let trimmed_start = line.trim_start();
                let leading = &line[..line.len() - trimmed_start.len()];
                let trimmed = trimmed_start.trim_end();
                let trailing = &trimmed_start[trimmed.len()..];
                format!("{}{}{}", leading, trimmed.to_case(case), trailing)
            })
            .join("\n")
    }

    pub fn convert_to_rot47(
        &mut self,
        _: &ConvertToRot47,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.manipulate_text(window, cx, |text| {
            text.chars()
                .map(|c| {
                    let code_point = c as u32;
                    if code_point >= 33 && code_point <= 126 {
                        return char::from_u32(33 + ((code_point + 14) % 94)).unwrap();
                    }
                    c
                })
                .collect()
        })
    }

    pub fn convert_to_base64(
        &mut self,
        _: &ConvertToBase64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use base64::Engine as _;
        self.manipulate_text(window, cx, |text| {
            base64::engine::general_purpose::STANDARD.encode(text)
        })
    }

    pub fn convert_from_base64(
        &mut self,
        _: &ConvertFromBase64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use base64::Engine as _;
        self.manipulate_text(
            window,
            cx,
            |text| match base64::engine::general_purpose::STANDARD.decode(text) {
                Ok(bytes) => String::from_utf8(bytes).unwrap_or_else(|_| text.to_string()),
                Err(_) => text.to_string(),
            },
        )
    }

    pub(crate) fn manipulate_text<Fn>(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        mut callback: Fn,
    ) where
        Fn: FnMut(&str) -> String,
    {
        if self.read_only(cx) {
            return;
        }
        let buffer = self.buffer.read(cx).snapshot(cx);

        let mut new_selections = Vec::new();
        let mut edits = Vec::new();

        for selection in self.selections.all_adjusted(&self.display_snapshot(cx)) {
            let selection_is_empty = selection.is_empty();

            let (start, end) = if selection_is_empty {
                let (word_range, _) = buffer.surrounding_word(selection.start, None);
                (word_range.start, word_range.end)
            } else {
                (
                    buffer.point_to_offset(selection.start),
                    buffer.point_to_offset(selection.end),
                )
            };

            let old_text = buffer.text_for_range(start..end).collect::<String>();
            let new_text = callback(&old_text);

            new_selections.push(Selection {
                start: buffer.anchor_before(start),
                end: buffer.anchor_after(end),
                goal: SelectionGoal::None,
                id: selection.id,
                reversed: selection.reversed,
            });

            if new_text != old_text {
                edits.push((start..end, new_text));
            }
        }

        if edits.is_empty() {
            return;
        }

        self.transact(window, cx, |this, window, cx| {
            this.buffer.update(cx, |buffer, cx| {
                buffer.edit(edits, None, cx);
            });

            this.change_selections(Default::default(), window, cx, |s| {
                s.select(new_selections);
            });

            this.request_autoscroll(Autoscroll::fit(), cx);
        });
    }
}
