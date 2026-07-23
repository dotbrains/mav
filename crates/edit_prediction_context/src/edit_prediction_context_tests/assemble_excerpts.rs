use super::test_support::*;
use super::*;

#[gpui::test]
async fn test_assemble_excerpts(cx: &mut TestAppContext) {
    let table = [
        (
            indoc! {r#"
                struct User {
                    first_name: String,
                    «last_name»: String,
                    age: u32,
                    email: String,
                    create_at: Instant,
                }

                impl User {
                    pub fn first_name(&self) -> String {
                        self.first_name.clone()
                    }

                    pub fn full_name(&self) -> String {
                «        format!("{} {}", self.first_name, self.last_name)
                »    }
                }
            "#},
            indoc! {r#"
                struct User {
                    first_name: String,
                    last_name: String,
                …
                }

                impl User {
                …
                    pub fn full_name(&self) -> String {
                        format!("{} {}", self.first_name, self.last_name)
                    }
                }
            "#},
        ),
        (
            indoc! {r#"
                struct «User» {
                    first_name: String,
                    last_name: String,
                    age: u32,
                }

                impl User {
                    // methods
                }
            "#},
            indoc! {r#"
                struct User {
                    first_name: String,
                    last_name: String,
                    age: u32,
                }
                …
            "#},
        ),
        (
            indoc! {r#"
                trait «FooProvider» {
                    const NAME: &'static str;

                    fn provide_foo(&self, id: usize) -> Foo;

                    fn provide_foo_batched(&self, ids: &[usize]) -> Vec<Foo> {
                            ids.iter()
                            .map(|id| self.provide_foo(*id))
                            .collect()
                    }

                    fn sync(&self);
                }
                "#
            },
            indoc! {r#"
                trait FooProvider {
                    const NAME: &'static str;

                    fn provide_foo(&self, id: usize) -> Foo;

                    fn provide_foo_batched(&self, ids: &[usize]) -> Vec<Foo> {
                …
                    }

                    fn sync(&self);
                }
            "#},
        ),
        (
            indoc! {r#"
                trait «Something» {
                    fn method1(&self, id: usize) -> Foo;

                    fn method2(&self, ids: &[usize]) -> Vec<Foo> {
                            struct Helper1 {
                            field1: usize,
                            }

                            struct Helper2 {
                            field2: usize,
                            }

                            struct Helper3 {
                            filed2: usize,
                        }
                    }

                    fn sync(&self);
                }
                "#
            },
            indoc! {r#"
                trait Something {
                    fn method1(&self, id: usize) -> Foo;

                    fn method2(&self, ids: &[usize]) -> Vec<Foo> {
                …
                    }

                    fn sync(&self);
                }
            "#},
        ),
    ];

    for (input, expected_output) in table {
        let (input, ranges) = marked_text_ranges(&input, false);
        let buffer = cx.new(|cx| Buffer::local(input, cx).with_language(rust_lang(), cx));
        buffer
            .read_with(cx, |buffer, _| buffer.parsing_idle())
            .await;
        buffer.read_with(cx, |buffer, _cx| {
            let ranges: Vec<(Range<Point>, usize)> = ranges
                .into_iter()
                .map(|range| (range.to_point(&buffer), 0))
                .collect();

            let assembled = assemble_excerpt_ranges(&buffer.snapshot(), ranges);
            let excerpts: Vec<RelatedExcerpt> = assembled
                .into_iter()
                .map(|(row_range, order)| {
                    let start = Point::new(row_range.start, 0);
                    let end = Point::new(row_range.end, buffer.line_len(row_range.end));
                    RelatedExcerpt {
                        row_range,
                        text: buffer.text_for_range(start..end).collect::<String>().into(),
                        order,
                        context_source: ContextSource::Lsp,
                    }
                })
                .collect();

            let output = format_excerpts(buffer, &excerpts);
            assert_eq!(output, expected_output);
        });
    }
}
