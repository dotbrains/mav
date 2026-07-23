use super::*;

pub(super) fn init_test(cx: &mut TestAppContext) {
    let settings_store = cx.update(|cx| SettingsStore::test(cx));
    cx.set_global(settings_store);
    env_logger::try_init().ok();
}

pub(super) fn setup_fake_lsp(
    project: &Entity<Project>,
    cx: &mut TestAppContext,
) -> UnboundedReceiver<FakeLanguageServer> {
    let (language_registry, fs) = project.read_with(cx, |project, _| {
        (project.languages().clone(), project.fs().clone())
    });
    let language = rust_lang();
    language_registry.add(language.clone());
    fake_definition_lsp::register_fake_definition_server(&language_registry, language, fs)
}

pub(super) fn test_project_1() -> serde_json::Value {
    let person_rs = indoc! {r#"
        pub struct Person {
            first_name: String,
            last_name: String,
            email: String,
            age: u32,
        }

        impl Person {
            pub fn get_first_name(&self) -> &str {
                &self.first_name
            }

            pub fn get_last_name(&self) -> &str {
                &self.last_name
            }

            pub fn get_email(&self) -> &str {
                &self.email
            }

            pub fn get_age(&self) -> u32 {
                self.age
            }
        }
    "#};

    let address_rs = indoc! {r#"
        pub struct Address {
            street: String,
            city: String,
            state: State,
            zip: u32,
        }

        pub enum State {
            CA,
            OR,
            WA,
            TX,
            // ...
        }

        impl Address {
            pub fn get_street(&self) -> &str {
                &self.street
            }

            pub fn get_city(&self) -> &str {
                &self.city
            }

            pub fn get_state(&self) -> State {
                self.state
            }

            pub fn get_zip(&self) -> u32 {
                self.zip
            }
        }
    "#};

    let company_rs = indoc! {r#"
        use super::person::Person;
        use super::address::Address;

        pub struct Company {
            owner: Arc<Person>,
            address: Address,
        }

        impl Company {
            pub fn get_owner(&self) -> &Person {
                &self.owner
            }

            pub fn get_address(&self) -> &Address {
                &self.address
            }

            pub fn to_string(&self) -> String {
                format!("{} ({})", self.owner.first_name, self.address.city)
            }
        }
    "#};

    let main_rs = indoc! {r#"
        use std::sync::Arc;
        use super::person::Person;
        use super::address::Address;
        use super::company::Company;

        pub struct Session {
            company: Arc<Company>,
        }

        impl Session {
            pub fn set_company(&mut self, company: Arc<Company>) {
                self.company = company;
                if company.owner != self.company.owner {
                    log("new owner", company.owner.get_first_name()); todo();
                }
            }
        }

        fn main() {
            let company = Company {
                owner: Arc::new(Person {
                    first_name: "John".to_string(),
                    last_name: "Doe".to_string(),
                    email: "john@example.com".to_string(),
                    age: 30,
                }),
                address: Address {
                    street: "123 Main St".to_string(),
                    city: "Anytown".to_string(),
                    state: State::CA,
                    zip: 12345,
                },
            };

            println!("Company: {}", company.to_string());
        }
    "#};

    json!({
        "src": {
            "person.rs": person_rs,
            "address.rs": address_rs,
            "company.rs": company_rs,
            "main.rs": main_rs,
        },
    })
}

pub(super) fn assert_related_files(
    actual_files: &[RelatedFile],
    expected_files: &[(&str, &[&str])],
) {
    let expected_with_orders: Vec<(&str, Vec<(&str, usize)>)> = expected_files
        .iter()
        .map(|(path, texts)| (*path, texts.iter().map(|text| (*text, 0)).collect()))
        .collect();
    let expected_refs: Vec<(&str, &[(&str, usize)])> = expected_with_orders
        .iter()
        .map(|(path, excerpts)| (*path, excerpts.as_slice()))
        .collect();
    assert_related_files_impl(actual_files, &expected_refs, false)
}

pub(super) fn assert_related_files_with_orders(
    actual_files: &[RelatedFile],
    expected_files: &[(&str, &[(&str, usize)])],
) {
    assert_related_files_impl(actual_files, expected_files, true)
}

pub(super) fn assert_related_files_impl(
    actual_files: &[RelatedFile],
    expected_files: &[(&str, &[(&str, usize)])],
    check_orders: bool,
) {
    let actual: Vec<(&str, Vec<(String, usize)>)> = actual_files
        .iter()
        .map(|file| {
            let excerpts = file
                .excerpts
                .iter()
                .map(|excerpt| {
                    let order = if check_orders { excerpt.order } else { 0 };
                    (excerpt.text.to_string(), order)
                })
                .collect();
            (file.path.to_str().unwrap(), excerpts)
        })
        .collect();
    let expected: Vec<(&str, Vec<(String, usize)>)> = expected_files
        .iter()
        .map(|(path, excerpts)| {
            (
                *path,
                excerpts
                    .iter()
                    .map(|(text, order)| (text.to_string(), *order))
                    .collect(),
            )
        })
        .collect();
    pretty_assertions::assert_eq!(actual, expected)
}

#[track_caller]
pub(super) fn assert_definitions(
    definitions: &[LocationLink],
    first_lines: &[&str],
    cx: &mut TestAppContext,
) {
    let actual_first_lines = definitions
        .iter()
        .map(|definition| {
            definition.target.buffer.read_with(cx, |buffer, _| {
                let mut start = definition.target.range.start.to_point(&buffer);
                start.column = 0;
                let end = Point::new(start.row, buffer.line_len(start.row));
                buffer
                    .text_for_range(start..end)
                    .collect::<String>()
                    .trim()
                    .to_string()
            })
        })
        .collect::<Vec<String>>();

    assert_eq!(actual_first_lines, first_lines);
}

pub(super) fn format_excerpts(buffer: &Buffer, excerpts: &[RelatedExcerpt]) -> String {
    let mut output = String::new();
    let file_line_count = buffer.max_point().row;
    let mut current_row = 0;
    for excerpt in excerpts {
        if excerpt.text.is_empty() {
            continue;
        }
        if current_row < excerpt.row_range.start {
            writeln!(&mut output, "…").unwrap();
        }
        current_row = excerpt.row_range.start;

        for line in excerpt.text.to_string().lines() {
            output.push_str(line);
            output.push('\n');
            current_row += 1;
        }
    }
    if current_row < file_line_count {
        writeln!(&mut output, "…").unwrap();
    }
    output
}
