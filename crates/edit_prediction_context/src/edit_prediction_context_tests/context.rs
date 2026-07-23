use super::test_support::*;
use super::*;

#[gpui::test]
async fn test_edit_prediction_context(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/root"), test_project_1()).await;

    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
    let mut servers = setup_fake_lsp(&project, cx);

    let (buffer, _handle) = project
        .update(cx, |project, cx| {
            project.open_local_buffer_with_lsp(path!("/root/src/main.rs"), cx)
        })
        .await
        .unwrap();

    let _server = servers.next().await.unwrap();
    cx.run_until_parked();

    let related_excerpt_store = cx.new(|cx| RelatedExcerptStore::new(&project, cx));
    related_excerpt_store.update(cx, |store, cx| {
        let position = {
            let buffer = buffer.read(cx);
            let offset = buffer.text().find("todo").unwrap();
            buffer.anchor_before(offset)
        };

        store.set_identifier_line_count(0);
        store.refresh(buffer.clone(), position, cx);
    });

    cx.executor().advance_clock(DEBOUNCE_DURATION);
    related_excerpt_store.update(cx, |store, cx| {
        let excerpts = store.related_files(cx);
        assert_related_files(
            &excerpts,
            &[
                (
                    "root/src/person.rs",
                    &[
                        indoc! {"
                        pub struct Person {
                            first_name: String,
                            last_name: String,
                            email: String,
                            age: u32,
                        }

                        impl Person {
                            pub fn get_first_name(&self) -> &str {
                                &self.first_name
                            }"},
                        "}",
                    ],
                ),
                (
                    "root/src/company.rs",
                    &[indoc! {"
                        pub struct Company {
                            owner: Arc<Person>,
                            address: Address,
                        }"}],
                ),
                (
                    "root/src/main.rs",
                    &[
                        indoc! {"
                        pub struct Session {
                            company: Arc<Company>,
                        }

                        impl Session {
                            pub fn set_company(&mut self, company: Arc<Company>) {"},
                        indoc! {"
                            }
                        }"},
                    ],
                ),
            ],
        );
    });

    let company_buffer = related_excerpt_store.update(cx, |store, cx| {
        store
            .related_files_with_buffers(cx)
            .find(|(file, _)| file.path.to_str() == Some("root/src/company.rs"))
            .map(|(_, buffer)| buffer)
            .expect("company.rs buffer not found")
    });

    company_buffer.update(cx, |buffer, cx| {
        let text = buffer.text();
        let insert_pos = text.find("address: Address,").unwrap() + "address: Address,".len();
        buffer.edit([(insert_pos..insert_pos, "\n    name: String,")], None, cx);
    });

    related_excerpt_store.update(cx, |store, cx| {
        let excerpts = store.related_files(cx);
        assert_related_files(
            &excerpts,
            &[
                (
                    "root/src/person.rs",
                    &[
                        indoc! {"
                        pub struct Person {
                            first_name: String,
                            last_name: String,
                            email: String,
                            age: u32,
                        }

                        impl Person {
                            pub fn get_first_name(&self) -> &str {
                                &self.first_name
                            }"},
                        "}",
                    ],
                ),
                (
                    "root/src/company.rs",
                    &[indoc! {"
                        pub struct Company {
                            owner: Arc<Person>,
                            address: Address,
                            name: String,
                        }"}],
                ),
                (
                    "root/src/main.rs",
                    &[
                        indoc! {"
                        pub struct Session {
                            company: Arc<Company>,
                        }

                        impl Session {
                            pub fn set_company(&mut self, company: Arc<Company>) {"},
                        indoc! {"
                            }
                        }"},
                    ],
                ),
            ],
        );
    });
}
