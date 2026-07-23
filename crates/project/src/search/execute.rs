use super::*;

impl SearchQuery {
    pub(crate) async fn detect(
        &self,
        mut reader: BufReader<Box<dyn Read + Send + Sync>>,
    ) -> Result<bool> {
        let query_str = self.as_str();
        if query_str.is_empty() {
            return Ok(false);
        }

        // Yield from this function every 20KB scanned.
        const YIELD_THRESHOLD: usize = 20 * 1024;

        match self {
            Self::Text { search, .. } => {
                let mut text = String::new();
                if query_str.contains('\n') {
                    reader.read_to_string(&mut text)?;
                    text::LineEnding::normalize(&mut text);
                    Ok(search.is_match(&text))
                } else {
                    let mut bytes_read = 0;
                    while reader.read_line(&mut text)? > 0 {
                        if search.is_match(&text) {
                            return Ok(true);
                        }
                        bytes_read += text.len();
                        if bytes_read >= YIELD_THRESHOLD {
                            bytes_read = 0;
                            smol::future::yield_now().await;
                        }
                        text.clear();
                    }
                    Ok(false)
                }
            }
            Self::Regex {
                regex, multiline, ..
            } => {
                let mut text = String::new();
                if *multiline {
                    reader.read_to_string(&mut text)?;
                    text::LineEnding::normalize(&mut text);
                    Ok(regex.is_match(&text)?)
                } else {
                    let mut bytes_read = 0;
                    while reader.read_line(&mut text)? > 0 {
                        if regex.is_match(&text)? {
                            return Ok(true);
                        }
                        bytes_read += text.len();
                        if bytes_read >= YIELD_THRESHOLD {
                            bytes_read = 0;
                            smol::future::yield_now().await;
                        }
                        text.clear();
                    }
                    Ok(false)
                }
            }
        }
    }

    pub async fn search(
        &self,
        buffer: &BufferSnapshot,
        subrange: Option<Range<usize>>,
    ) -> Vec<Range<usize>> {
        const YIELD_INTERVAL: usize = 20000;

        if self.as_str().is_empty() {
            return Default::default();
        }

        let range_offset = subrange.as_ref().map(|r| r.start).unwrap_or(0);
        let rope = if let Some(range) = subrange {
            buffer.as_rope().slice(range)
        } else {
            buffer.as_rope().clone()
        };

        let mut matches = Vec::new();
        match self {
            Self::Text {
                search, whole_word, ..
            } => {
                for (ix, mat) in search
                    .stream_find_iter(rope.bytes_in_range(0..rope.len()))
                    .enumerate()
                {
                    if (ix + 1) % YIELD_INTERVAL == 0 {
                        yield_now().await;
                    }

                    let mat = mat.unwrap();
                    if *whole_word {
                        let classifier = buffer.char_classifier_at(range_offset + mat.start());

                        let prev_kind = rope
                            .reversed_chars_at(mat.start())
                            .next()
                            .map(|c| classifier.kind(c));
                        let start_kind =
                            classifier.kind(rope.chars_at(mat.start()).next().unwrap());
                        let end_kind =
                            classifier.kind(rope.reversed_chars_at(mat.end()).next().unwrap());
                        let next_kind = rope.chars_at(mat.end()).next().map(|c| classifier.kind(c));
                        if (Some(start_kind) == prev_kind && start_kind == CharKind::Word)
                            || (Some(end_kind) == next_kind && end_kind == CharKind::Word)
                        {
                            continue;
                        }
                    }
                    matches.push(mat.start()..mat.end())
                }
            }

            Self::Regex {
                regex, multiline, ..
            } => {
                if *multiline {
                    let text = rope.to_string();
                    for (ix, mat) in regex.find_iter(&text).enumerate() {
                        if (ix + 1) % YIELD_INTERVAL == 0 {
                            yield_now().await;
                        }

                        if let Ok(mat) = mat {
                            matches.push(mat.start()..mat.end());
                        }
                    }
                } else {
                    let mut line = String::new();
                    let mut line_offset = 0;
                    for (chunk_ix, chunk) in rope.chunks().chain(["\n"]).enumerate() {
                        if (chunk_ix + 1) % YIELD_INTERVAL == 0 {
                            yield_now().await;
                        }

                        for (newline_ix, text) in chunk.split('\n').enumerate() {
                            if newline_ix > 0 {
                                for mat in regex.find_iter(&line).flatten() {
                                    let start = line_offset + mat.start();
                                    let end = line_offset + mat.end();
                                    matches.push(start..end);
                                    if self.one_match_per_line() == Some(true) {
                                        break;
                                    }
                                }

                                line_offset += line.len() + 1;
                                line.clear();
                            }
                            line.push_str(text);
                        }
                    }
                }
            }
        }

        matches
    }

    pub fn search_str(&self, text: &str) -> Vec<Range<usize>> {
        if self.as_str().is_empty() {
            return Vec::new();
        }

        let is_word_char = |c: char| c.is_alphanumeric() || c == '_';

        let mut matches = Vec::new();
        match self {
            Self::Text {
                search, whole_word, ..
            } => {
                for mat in search.find_iter(text.as_bytes()) {
                    if *whole_word {
                        let prev_char = text[..mat.start()].chars().last();
                        let next_char = text[mat.end()..].chars().next();
                        if prev_char.is_some_and(&is_word_char)
                            || next_char.is_some_and(&is_word_char)
                        {
                            continue;
                        }
                    }
                    matches.push(mat.start()..mat.end());
                }
            }
            Self::Regex {
                regex,
                multiline,
                one_match_per_line,
                ..
            } => {
                if *multiline {
                    for mat in regex.find_iter(text).flatten() {
                        matches.push(mat.start()..mat.end());
                    }
                } else {
                    let mut line_offset = 0;
                    for line in text.split('\n') {
                        for mat in regex.find_iter(line).flatten() {
                            matches.push((line_offset + mat.start())..(line_offset + mat.end()));
                            if *one_match_per_line {
                                break;
                            }
                        }
                        line_offset += line.len() + 1;
                    }
                }
            }
        }
        matches
    }
}
