use super::*;

#[test]
#[cfg_attr(not(feature = "unit-eval"), ignore)]
fn eval_create_file() {
    let input_file_path = "root/TODO3";
    let expected_output_content = "todo".to_string();

    eval_utils::eval(100, 1., eval_utils::NoProcessor, move || {
        run_eval(EvalInput::new(
            vec![
                message(
                    User,
                    [text("Create a third todo file. Write 'todo' inside it.")],
                ),
                message(
                    Assistant,
                    [
                        text(indoc::formatdoc! {"
                            I'll help you create a third empty todo file.
                            First, let me examine the project structure to see if there's already a todo file, which will help me determine the appropriate name and location for the second one.
                            "}),
                        tool_use(
                            "toolu_01GAF8TtsgpjKxCr8fgQLDgR",
                            ListDirectoryTool::NAME,
                            ListDirectoryToolInput {
                                path: "root".to_string(),
                            },
                        ),
                    ],
                ),
                message(
                    User,
                    [tool_result(
                        "toolu_01GAF8TtsgpjKxCr8fgQLDgR",
                        ListDirectoryTool::NAME,
                        "root/TODO\nroot/TODO2\nroot/new.txt\n",
                    )],
                ),
            ],
            input_file_path,
            None,
            expected_output_content.clone(),
        ))
    });
}

#[test]
#[cfg_attr(not(feature = "unit-eval"), ignore)]
fn eval_overwrite_file() {
    let input_file_path = "root/notes.txt";
    let input_file_content = "old notes\nkeep nothing\n".to_string();
    let expected_output_content = "new notes".to_string();

    eval_utils::eval(100, 1., eval_utils::NoProcessor, move || {
        run_eval(EvalInput::new(
            vec![message(
                User,
                [text(indoc::formatdoc! {"
                    Overwrite `{input_file_path}` so that its complete contents are exactly: 'new notes'
                "})],
            )],
            input_file_path,
            Some(input_file_content.clone()),
            expected_output_content.clone(),
        ))
    });
}
