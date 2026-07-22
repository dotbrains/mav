use std::path::{Path, PathBuf};

use crate::{
    VimAddon,
    state::Mode,
    test::{NeovimBackedTestContext, VimTestContext},
};
use editor::{Editor, EditorSettings};
use gpui::{Context, TestAppContext};
use indoc::indoc;
use settings::Settings;
use util::path;
use workspace::{OpenOptions, Workspace};

#[gpui::test]
async fn test_command_basics(cx: &mut TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {"
            ˇa
            b
            c"})
        .await;

    cx.simulate_shared_keystrokes(": j enter").await;

    // hack: our cursor positioning after a join command is wrong
    cx.simulate_shared_keystrokes("^").await;
    cx.shared_state().await.assert_eq(indoc! {
        "ˇa b
        c"
    });
}

#[gpui::test]
async fn test_command_goto(cx: &mut TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {"
            ˇa
            b
            c"})
        .await;
    cx.simulate_shared_keystrokes(": 3 enter").await;
    cx.shared_state().await.assert_eq(indoc! {"
            a
            b
            ˇc"});
}

#[gpui::test]
async fn test_command_replace(cx: &mut TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {"
            ˇa
            b
            b
            c"})
        .await;
    cx.simulate_shared_keystrokes(": % s / b / d enter").await;
    cx.shared_state().await.assert_eq(indoc! {"
            a
            d
            ˇd
            c"});
    cx.simulate_shared_keystrokes(": % s : . : \\ 0 \\ 0 enter")
        .await;
    cx.shared_state().await.assert_eq(indoc! {"
            aa
            dd
            dd
            ˇcc"});
    cx.simulate_shared_keystrokes("k : s / d d / e e enter")
        .await;
    cx.shared_state().await.assert_eq(indoc! {"
            aa
            dd
            ˇee
            cc"});
}

#[gpui::test]
async fn test_command_search(cx: &mut TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {"
                ˇa
                b
                a
                c"})
        .await;
    cx.simulate_shared_keystrokes(": / b enter").await;
    cx.shared_state().await.assert_eq(indoc! {"
                a
                ˇb
                a
                c"});
    cx.simulate_shared_keystrokes(": ? a enter").await;
    cx.shared_state().await.assert_eq(indoc! {"
                ˇa
                b
                a
                c"});
}

#[gpui::test]
async fn test_command_write(cx: &mut TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    let path = Path::new(path!("/root/dir/file.rs"));
    let fs = cx.workspace(|workspace, _, cx| workspace.project().read(cx).fs().clone());

    cx.simulate_keystrokes("i @ escape");
    cx.simulate_keystrokes(": w enter");

    assert_eq!(fs.load(path).await.unwrap().replace("\r\n", "\n"), "@\n");

    fs.as_fake().insert_file(path, b"oops\n".to_vec()).await;

    // conflict!
    cx.simulate_keystrokes("i @ escape");
    cx.simulate_keystrokes(": w enter");
    cx.simulate_prompt_answer("Cancel");

    assert_eq!(fs.load(path).await.unwrap().replace("\r\n", "\n"), "oops\n");
    assert!(!cx.has_pending_prompt());
    cx.simulate_keystrokes(": w !");
    cx.simulate_keystrokes("enter");
    assert!(!cx.has_pending_prompt());
    assert_eq!(fs.load(path).await.unwrap().replace("\r\n", "\n"), "@@\n");
}

#[gpui::test]
async fn test_command_read(cx: &mut TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    let fs = cx.workspace(|workspace, _, cx| workspace.project().read(cx).fs().clone());
    let path = Path::new(path!("/root/dir/other.rs"));
    fs.as_fake().insert_file(path, "1\n2\n3".into()).await;

    cx.workspace(|workspace, _, cx| {
        assert_active_item(workspace, path!("/root/dir/file.rs"), "", cx);
    });

    // File without trailing newline
    cx.set_state("one\ntwo\nthreeˇ", Mode::Normal);
    cx.simulate_keystrokes(": r space d i r / o t h e r . r s");
    cx.simulate_keystrokes("enter");
    cx.assert_state("one\ntwo\nthree\nˇ1\n2\n3", Mode::Normal);

    cx.set_state("oneˇ\ntwo\nthree", Mode::Normal);
    cx.simulate_keystrokes(": r space d i r / o t h e r . r s");
    cx.simulate_keystrokes("enter");
    cx.assert_state("one\nˇ1\n2\n3\ntwo\nthree", Mode::Normal);

    cx.set_state("one\nˇtwo\nthree", Mode::Normal);
    cx.simulate_keystrokes(": 0 r space d i r / o t h e r . r s");
    cx.simulate_keystrokes("enter");
    cx.assert_state("ˇ1\n2\n3\none\ntwo\nthree", Mode::Normal);

    cx.set_state("one\n«ˇtwo\nthree\nfour»\nfive", Mode::Visual);
    cx.simulate_keystrokes(": r space d i r / o t h e r . r s");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    cx.assert_state("one\ntwo\nthree\nfour\nˇ1\n2\n3\nfive", Mode::Normal);

    // Empty filename
    cx.set_state("oneˇ\ntwo\nthree", Mode::Normal);
    cx.simulate_keystrokes(": r");
    cx.simulate_keystrokes("enter");
    cx.assert_state("one\nˇone\ntwo\nthree\ntwo\nthree", Mode::Normal);

    // File with trailing newline
    fs.as_fake().insert_file(path, "1\n2\n3\n".into()).await;
    cx.set_state("one\ntwo\nthreeˇ", Mode::Normal);
    cx.simulate_keystrokes(": r space d i r / o t h e r . r s");
    cx.simulate_keystrokes("enter");
    cx.assert_state("one\ntwo\nthree\nˇ1\n2\n3\n", Mode::Normal);

    cx.set_state("oneˇ\ntwo\nthree", Mode::Normal);
    cx.simulate_keystrokes(": r space d i r / o t h e r . r s");
    cx.simulate_keystrokes("enter");
    cx.assert_state("one\nˇ1\n2\n3\n\ntwo\nthree", Mode::Normal);

    cx.set_state("one\n«ˇtwo\nthree\nfour»\nfive", Mode::Visual);
    cx.simulate_keystrokes(": r space d i r / o t h e r . r s");
    cx.simulate_keystrokes("enter");
    cx.assert_state("one\ntwo\nthree\nfour\nˇ1\n2\n3\n\nfive", Mode::Normal);

    cx.set_state("«one\ntwo\nthreeˇ»", Mode::Visual);
    cx.simulate_keystrokes(": r space d i r / o t h e r . r s");
    cx.simulate_keystrokes("enter");
    cx.assert_state("one\ntwo\nthree\nˇ1\n2\n3\n", Mode::Normal);

    // Empty file
    fs.as_fake().insert_file(path, "".into()).await;
    cx.set_state("ˇone\ntwo\nthree", Mode::Normal);
    cx.simulate_keystrokes(": r space d i r / o t h e r . r s");
    cx.simulate_keystrokes("enter");
    cx.assert_state("one\nˇtwo\nthree", Mode::Normal);
}

#[gpui::test]
async fn test_command_quit(cx: &mut TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.simulate_keystrokes(": n e w enter");
    cx.workspace(|workspace, _, cx| assert_eq!(workspace.items(cx).count(), 2));
    cx.simulate_keystrokes(": q enter");
    cx.workspace(|workspace, _, cx| assert_eq!(workspace.items(cx).count(), 1));
    cx.simulate_keystrokes(": n e w enter");
    cx.workspace(|workspace, _, cx| assert_eq!(workspace.items(cx).count(), 2));
    cx.simulate_keystrokes(": q a enter");
    cx.workspace(|workspace, _, cx| assert_eq!(workspace.items(cx).count(), 0));
}

#[gpui::test]
async fn test_offsets(cx: &mut TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state("ˇ1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n11\n")
        .await;

    cx.simulate_shared_keystrokes(": + enter").await;
    cx.shared_state()
        .await
        .assert_eq("1\nˇ2\n3\n4\n5\n6\n7\n8\n9\n10\n11\n");

    cx.simulate_shared_keystrokes(": 1 0 - enter").await;
    cx.shared_state()
        .await
        .assert_eq("1\n2\n3\n4\n5\n6\n7\n8\nˇ9\n10\n11\n");

    cx.simulate_shared_keystrokes(": . - 2 enter").await;
    cx.shared_state()
        .await
        .assert_eq("1\n2\n3\n4\n5\n6\nˇ7\n8\n9\n10\n11\n");

    cx.simulate_shared_keystrokes(": % enter").await;
    cx.shared_state()
        .await
        .assert_eq("1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n11\nˇ");
}

#[gpui::test]
async fn test_command_ranges(cx: &mut TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state("ˇ1\n2\n3\n4\n4\n3\n2\n1").await;

    cx.simulate_shared_keystrokes(": 2 , 4 d enter").await;
    cx.shared_state().await.assert_eq("1\nˇ4\n3\n2\n1");

    cx.simulate_shared_keystrokes(": 2 , 4 s o r t enter").await;
    cx.shared_state().await.assert_eq("1\nˇ2\n3\n4\n1");

    cx.simulate_shared_keystrokes(": 2 , 4 j o i n enter").await;
    cx.shared_state().await.assert_eq("1\nˇ2 3 4\n1");
}

#[gpui::test]
async fn test_command_visual_replace(cx: &mut TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state("ˇ1\n2\n3\n4\n4\n3\n2\n1").await;

    cx.simulate_shared_keystrokes("v 2 j : s / . / k enter")
        .await;
    cx.shared_state().await.assert_eq("k\nk\nˇk\n4\n4\n3\n2\n1");
}

#[path = "test_file_commands.rs"]
mod file_commands;

#[gpui::test]
async fn test_command_matching_lines(cx: &mut TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {"
            ˇa
            b
            a
            b
            a
        "})
        .await;

    cx.simulate_shared_keystrokes(":").await;
    cx.simulate_shared_keystrokes("g / a / d").await;
    cx.simulate_shared_keystrokes("enter").await;

    cx.shared_state().await.assert_eq(indoc! {"
            b
            b
            ˇ"});

    cx.simulate_shared_keystrokes("u").await;

    cx.shared_state().await.assert_eq(indoc! {"
            ˇa
            b
            a
            b
            a
        "});

    cx.simulate_shared_keystrokes(":").await;
    cx.simulate_shared_keystrokes("v / a / d").await;
    cx.simulate_shared_keystrokes("enter").await;

    cx.shared_state().await.assert_eq(indoc! {"
            a
            a
            ˇa"});
}

#[gpui::test]
async fn test_del_marks(cx: &mut TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {"
            ˇa
            b
            a
            b
            a
        "})
        .await;

    cx.simulate_shared_keystrokes("m a").await;

    let mark = cx.update_editor(|editor, window, cx| {
        let vim = editor.addon::<VimAddon>().unwrap().entity.clone();
        vim.update(cx, |vim, cx| vim.get_mark("a", editor, window, cx))
    });
    assert!(mark.is_some());

    cx.simulate_shared_keystrokes(": d e l m space a").await;
    cx.simulate_shared_keystrokes("enter").await;

    let mark = cx.update_editor(|editor, window, cx| {
        let vim = editor.addon::<VimAddon>().unwrap().entity.clone();
        vim.update(cx, |vim, cx| vim.get_mark("a", editor, window, cx))
    });
    assert!(mark.is_none())
}

#[gpui::test]
async fn test_normal_command(cx: &mut TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {"
            The quick
            brown« fox
            jumpsˇ» over
            the lazy dog
        "})
        .await;

    cx.simulate_shared_keystrokes(": n o r m space w C w o r d")
        .await;
    cx.simulate_shared_keystrokes("enter").await;

    cx.shared_state().await.assert_eq(indoc! {"
            The quick
            brown word
            jumps worˇd
            the lazy dog
        "});

    cx.simulate_shared_keystrokes(": n o r m space _ w c i w t e s t")
        .await;
    cx.simulate_shared_keystrokes("enter").await;

    cx.shared_state().await.assert_eq(indoc! {"
            The quick
            brown word
            jumps tesˇt
            the lazy dog
        "});

    cx.simulate_shared_keystrokes("_ l v l : n o r m space s l a")
        .await;
    cx.simulate_shared_keystrokes("enter").await;

    cx.shared_state().await.assert_eq(indoc! {"
            The quick
            brown word
            lˇaumps test
            the lazy dog
        "});

    cx.set_shared_state(indoc! {"
            ˇThe quick
            brown fox
            jumps over
            the lazy dog
        "})
        .await;

    cx.simulate_shared_keystrokes("c i w M y escape").await;

    cx.shared_state().await.assert_eq(indoc! {"
            Mˇy quick
            brown fox
            jumps over
            the lazy dog
        "});

    cx.simulate_shared_keystrokes(": n o r m space u").await;
    cx.simulate_shared_keystrokes("enter").await;

    cx.shared_state().await.assert_eq(indoc! {"
            ˇThe quick
            brown fox
            jumps over
            the lazy dog
        "});

    cx.set_shared_state(indoc! {"
            The« quick
            brownˇ» fox
            jumps over
            the lazy dog
        "})
        .await;

    cx.simulate_shared_keystrokes(": n o r m space I 1 2 3")
        .await;
    cx.simulate_shared_keystrokes("enter").await;
    cx.simulate_shared_keystrokes("u").await;

    cx.shared_state().await.assert_eq(indoc! {"
            ˇThe quick
            brown fox
            jumps over
            the lazy dog
        "});

    cx.set_shared_state(indoc! {"
            ˇquick
            brown fox
            jumps over
            the lazy dog
        "})
        .await;

    cx.simulate_shared_keystrokes(": n o r m space I T h e space")
        .await;
    cx.simulate_shared_keystrokes("enter").await;

    cx.shared_state().await.assert_eq(indoc! {"
            Theˇ quick
            brown fox
            jumps over
            the lazy dog
        "});

    // Once ctrl-v to input character literals is added there should be a test for redo
}

#[gpui::test]
async fn test_command_g_normal(cx: &mut TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {"
            ˇfoo

            foo
        "})
        .await;

    cx.simulate_shared_keystrokes(": % g / f o o / n o r m space A b a r")
        .await;
    cx.simulate_shared_keystrokes("enter").await;
    cx.run_until_parked();

    cx.shared_state().await.assert_eq(indoc! {"
            foobar

            foobaˇr
        "});

    cx.simulate_shared_keystrokes("u").await;

    cx.shared_state().await.assert_eq(indoc! {"
            foˇo

            foo
        "});
}

#[path = "tests_editing.rs"]
mod editing;
