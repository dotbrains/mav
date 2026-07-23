use super::test_support::*;
use super::*;

#[gpui::test]
async fn test_fake_definition_lsp(cx: &mut TestAppContext) {
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

    let buffer_text = buffer.read_with(cx, |buffer, _| buffer.text());

    let definitions = project
        .update(cx, |project, cx| {
            let offset = buffer_text.find("Address {").unwrap();
            project.definitions(&buffer, offset, cx)
        })
        .await
        .unwrap()
        .unwrap();
    assert_definitions(&definitions, &["pub struct Address {"], cx);

    let definitions = project
        .update(cx, |project, cx| {
            let offset = buffer_text.find("State::CA").unwrap();
            project.definitions(&buffer, offset, cx)
        })
        .await
        .unwrap()
        .unwrap();
    assert_definitions(&definitions, &["pub enum State {"], cx);

    let definitions = project
        .update(cx, |project, cx| {
            let offset = buffer_text.find("to_string()").unwrap();
            project.definitions(&buffer, offset, cx)
        })
        .await
        .unwrap()
        .unwrap();
    assert_definitions(&definitions, &["pub fn to_string(&self) -> String {"], cx);
}

#[gpui::test]
async fn test_fake_type_definition_lsp(cx: &mut TestAppContext) {
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

    let buffer_text = buffer.read_with(cx, |buffer, _| buffer.text());

    // Type definition on a type name returns its own definition
    // (same as regular definition)
    let type_defs = project
        .update(cx, |project, cx| {
            let offset = buffer_text.find("Address {").expect("Address { not found");
            project.type_definitions(&buffer, offset, cx)
        })
        .await
        .unwrap()
        .unwrap();
    assert_definitions(&type_defs, &["pub struct Address {"], cx);

    // Type definition on a field resolves through the type annotation.
    // company.rs has `owner: Arc<Person>`, so type-def of `owner` → Person.
    let (company_buffer, _handle) = project
        .update(cx, |project, cx| {
            project.open_local_buffer_with_lsp(path!("/root/src/company.rs"), cx)
        })
        .await
        .unwrap();
    cx.run_until_parked();

    let company_text = company_buffer.read_with(cx, |buffer, _| buffer.text());
    let type_defs = project
        .update(cx, |project, cx| {
            let offset = company_text.find("owner").expect("owner not found");
            project.type_definitions(&company_buffer, offset, cx)
        })
        .await
        .unwrap()
        .unwrap();
    assert_definitions(&type_defs, &["pub struct Person {"], cx);

    // Type definition on another field: `address: Address` → Address.
    let type_defs = project
        .update(cx, |project, cx| {
            let offset = company_text.find("address").expect("address not found");
            project.type_definitions(&company_buffer, offset, cx)
        })
        .await
        .unwrap()
        .unwrap();
    assert_definitions(&type_defs, &["pub struct Address {"], cx);

    // Type definition on a lowercase name with no type annotation returns empty.
    let type_defs = project
        .update(cx, |project, cx| {
            let offset = buffer_text.find("main").expect("main not found");
            project.type_definitions(&buffer, offset, cx)
        })
        .await;
    let is_empty = match &type_defs {
        Ok(Some(defs)) => defs.is_empty(),
        Ok(None) => true,
        Err(_) => false,
    };
    assert!(is_empty, "expected no type definitions for `main`");
}
