
/// 👉 := **hovered** on following char
///
/// 👈 := **hovered** on wide char spacer of previous full width char
///
/// **`‹›`** := expected **hyperlink** match
///
/// **`«»`** := expected **path**, **row**, and **column** capture groups
///
/// [**`c₀, c₁, …, cₙ;`**]ₒₚₜ := use specified terminal widths of `c₀, c₁, …, cₙ` **columns**
/// (defaults to `3, longest_line_cells / 2, longest_line_cells + 1;`)
///
macro_rules! test_path {
            ($($lines:literal),+) => { test_hyperlink!($($lines),+; Path) };
        }

#[test]
fn simple() {
    // Rust paths
    // Just the path
    test_path!("‹«/👉test/cool.rs»›");
    test_path!("‹«/test/cool👉.rs»›");

    // path and line
    test_path!("‹«/👉test/cool.rs»:«4»›");
    test_path!("‹«/test/cool.rs»👉:«4»›");
    test_path!("‹«/test/cool.rs»:«👉4»›");
    test_path!("‹«/👉test/cool.rs»(«4»)›");
    test_path!("‹«/test/cool.rs»👉(«4»)›");
    test_path!("‹«/test/cool.rs»(«👉4»)›");
    test_path!("‹«/test/cool.rs»(«4»👉)›");

    // path, line, and column
    test_path!("‹«/👉test/cool.rs»:«4»:«2»›");
    test_path!("‹«/test/cool.rs»:«4»:«👉2»›");
    test_path!("‹«/👉test/cool.rs»(«4»,«2»)›");
    test_path!("‹«/test/cool.rs»(«4»👉,«2»)›");

    // path, line, column, and ':' suffix
    test_path!("‹«/👉test/cool.rs»:«4»:«2»›:");
    test_path!("‹«/test/cool.rs»:«4»:«👉2»›:");
    test_path!("‹«/👉test/cool.rs»(«4»,«2»)›:");
    test_path!("‹«/test/cool.rs»(«4»,«2»👉)›:");
    test_path!("‹«/👉test/cool.rs»:(«4»,«2»)›:");
    test_path!("‹«/test/cool.rs»:(«4»,«2»👉)›:");
    test_path!("‹«/👉test/cool.rs»:(«4»:«2»)›:");
    test_path!("‹«/test/cool.rs»:(«4»:«2»👉)›:");
    test_path!("/test/cool.rs:4:2👉:", "What is this?");
    test_path!("/test/cool.rs(4,2)👉:", "What is this?");

    // path, line, column, and description
    test_path!("‹«/test/co👉ol.rs»:«4»:«2»›:Error!");
    test_path!("‹«/test/co👉ol.rs»(«4»,«2»)›:Error!");

    // Cargo output
    test_path!("    Compiling Cool 👉(/test/Cool)");
    test_path!("    Compiling Cool (‹«/👉test/Cool»›)");
    test_path!("    Compiling Cool (/test/Cool👉)");

    // Tool output with path inside parens (e.g. Claude Code)
    test_path!("Update👉(src/cool.rs)");
    test_path!("Update(‹«src/👉cool.rs»›)");
    test_path!("Update(src/cool.rs👉)");
    test_path!("Write(‹«/👉test/Cool»›)");

    // Python
    test_path!("‹«awe👉some.py»›");
    test_path!("‹«👉a»› ");

    test_path!("    ‹F👉ile \"«/awesome.py»\", line «42»›: Wat?");
    test_path!("    ‹File \"«/awe👉some.py»\", line «42»›");
    test_path!("    ‹File \"«/awesome.py»👉\", line «42»›: Wat?");
    test_path!("    ‹File \"«/awesome.py»\", line «4👉2»›");
}

#[test]
fn simple_with_descriptions() {
    // path, line, column and description
    test_path!("‹«/👉test/cool.rs»:«4»:«2»›:例Desc例例例");
    test_path!("‹«/test/cool.rs»:«4»:«👉2»›:例Desc例例例");
    test_path!("‹«/👉test/cool.rs»(«4»,«2»)›:例Desc例例例");
    test_path!("‹«/test/cool.rs»(«4»👉,«2»)›:例Desc例例例");

    // path, line, column and description w/extra colons
    test_path!("‹«/👉test/cool.rs»:«4»:«2»›::例Desc例例例");
    test_path!("‹«/test/cool.rs»:«4»:«👉2»›::例Desc例例例");
    test_path!("‹«/👉test/cool.rs»(«4»,«2»)›::例Desc例例例");
    test_path!("‹«/test/cool.rs»(«4»,«2»👉)›::例Desc例例例");
}

#[test]
fn multiple_same_line() {
    test_path!("‹«/👉test/cool.rs»› /test/cool.rs");
    test_path!("/test/cool.rs ‹«/👉test/cool.rs»›");

    test_path!("‹«🦀 multiple_👉same_line 🦀» 🚣«4» 🏛️«2»›: 🦀 multiple_same_line 🦀 🚣4 🏛️2:");

    // ls output (tab separated)
    test_path!("‹«Carg👉o.toml»›\t\texperiments\t\tnotebooks\t\trust-toolchain.toml\ttooling");
    test_path!("Cargo.toml\t\t‹«exper👉iments»›\t\tnotebooks\t\trust-toolchain.toml\ttooling");
    test_path!("Cargo.toml\t\texperiments\t\t‹«note👉books»›\t\trust-toolchain.toml\ttooling");
    test_path!("Cargo.toml\t\texperiments\t\tnotebooks\t\t‹«rust-t👉oolchain.toml»›\ttooling");
    test_path!("Cargo.toml\t\texperiments\t\tnotebooks\t\trust-toolchain.toml\t‹«too👉ling»›");
}

#[test]
fn colons_galore() {
    test_path!("‹«/test/co👉ol.rs»:«4»›");
    test_path!("‹«/test/co👉ol.rs»:«4»›:");
    test_path!("‹«/test/co👉ol.rs»:«4»:«2»›");
    test_path!("‹«/test/co👉ol.rs»:«4»:«2»›:");
    test_path!("‹«/test/co👉ol.rs»(«1»)›");
    test_path!("‹«/test/co👉ol.rs»(«1»)›:");
    test_path!("‹«/test/co👉ol.rs»(«1»,«618»)›");
    test_path!("‹«/test/co👉ol.rs»(«1»,«618»)›:");
    test_path!("‹«/test/co👉ol.rs»::«42»›");
    test_path!("‹«/test/co👉ol.rs»::«42»›:");
    test_path!("‹«/test/co👉ol.rs»(«1»,«618»)›::");
}

#[test]
fn quotes_and_brackets() {
    test_path!("\"‹«/test/co👉ol.rs»:«4»›\"");
    test_path!("'‹«/test/co👉ol.rs»:«4»›'");
    test_path!("`‹«/test/co👉ol.rs»:«4»›`");

    test_path!("[‹«/test/co👉ol.rs»:«4»›]");
    test_path!("(‹«/test/co👉ol.rs»:«4»›)");
    test_path!("{‹«/test/co👉ol.rs»:«4»›}");
    test_path!("<‹«/test/co👉ol.rs»:«4»›>");

    test_path!("[\"‹«/test/co👉ol.rs»:«4»›\"]");
    test_path!("'(‹«/test/co👉ol.rs»:«4»›)'");

    test_path!("\"‹«/test/co👉ol.rs»:«4»:«2»›\"");
    test_path!("'‹«/test/co👉ol.rs»:«4»:«2»›'");
    test_path!("`‹«/test/co👉ol.rs»:«4»:«2»›`");

    test_path!("[‹«/test/co👉ol.rs»:«4»:«2»›]");
    test_path!("(‹«/test/co👉ol.rs»:«4»:«2»›)");
    test_path!("{‹«/test/co👉ol.rs»:«4»:«2»›}");
    test_path!("<‹«/test/co👉ol.rs»:«4»:«2»›>");

    test_path!("[\"‹«/test/co👉ol.rs»:«4»:«2»›\"]");

    test_path!("\"‹«/test/co👉ol.rs»(«4»)›\"");
    test_path!("'‹«/test/co👉ol.rs»(«4»)›'");
    test_path!("`‹«/test/co👉ol.rs»(«4»)›`");

    test_path!("[‹«/test/co👉ol.rs»(«4»)›]");
    test_path!("(‹«/test/co👉ol.rs»(«4»)›)");
    test_path!("{‹«/test/co👉ol.rs»(«4»)›}");
    test_path!("<‹«/test/co👉ol.rs»(«4»)›>");

    test_path!("[\"‹«/test/co👉ol.rs»(«4»)›\"]");

    test_path!("\"‹«/test/co👉ol.rs»(«4»,«2»)›\"");
    test_path!("'‹«/test/co👉ol.rs»(«4»,«2»)›'");
    test_path!("`‹«/test/co👉ol.rs»(«4»,«2»)›`");

    test_path!("[‹«/test/co👉ol.rs»(«4»,«2»)›]");
    test_path!("(‹«/test/co👉ol.rs»(«4»,«2»)›)");
    test_path!("{‹«/test/co👉ol.rs»(«4»,«2»)›}");
    test_path!("<‹«/test/co👉ol.rs»(«4»,«2»)›>");

    test_path!("[\"‹«/test/co👉ol.rs»(«4»,«2»)›\"]");

    // Imbalanced
    test_path!("([‹«/test/co👉ol.rs»:«4»›] was here...)");
    test_path!("[Here's <‹«/test/co👉ol.rs»:«4»›>]");
    test_path!("('‹«/test/co👉ol.rs»:«4»›' was here...)");
    test_path!("[Here's `‹«/test/co👉ol.rs»:«4»›`]");
}

#[test]
fn trailing_punctuation() {
    test_path!("‹«/test/co👉ol.rs»›:,..");
    test_path!("/test/cool.rs:,👉..");
    test_path!("‹«/test/co👉ol.rs»:«4»›:,");
    test_path!("/test/cool.rs:4:👉,");
    test_path!("[\"‹«/test/co👉ol.rs»:«4»›\"]:,");
    test_path!("'(‹«/test/co👉ol.rs»:«4»›),,'...");
    test_path!("('‹«/test/co👉ol.rs»:«4»›'::: was here...)");
    test_path!("[Here's <‹«/test/co👉ol.rs»:«4»›>]::: ");
}

#[test]
fn word_wide_chars() {
    // Rust paths
    test_path!("‹«/👉例/cool.rs»›");
    test_path!("‹«/例👈/cool.rs»›");
    test_path!("‹«/例/cool.rs»:«👉4»›");
    test_path!("‹«/例/cool.rs»:«4»:«👉2»›");

    // Cargo output
    test_path!("    Compiling Cool (‹«/👉例/Cool»›)");
    test_path!("    Compiling Cool (‹«/例👈/Cool»›)");

    test_path!("    Compiling Cool (‹«/👉例/Cool Spaces»›)");
    test_path!("    Compiling Cool (‹«/例👈/Cool Spaces»›)");
    test_path!("    Compiling Cool (‹«/👉例/Cool Spaces»:«4»:«2»›)");
    test_path!("    Compiling Cool (‹«/例👈/Cool Spaces»(«4»,«2»)›)");

    test_path!("    --> ‹«/👉例/Cool Spaces»›");
    test_path!("    ::: ‹«/例👈/Cool Spaces»›");
    test_path!("    --> ‹«/👉例/Cool Spaces»:«4»:«2»›");
    test_path!("    ::: ‹«/例👈/Cool Spaces»(«4»,«2»)›");
    test_path!("    panicked at ‹«/👉例/Cool Spaces»:«4»:«2»›:");
    test_path!("    panicked at ‹«/例👈/Cool Spaces»(«4»,«2»)›:");
    test_path!("    at ‹«/👉例/Cool Spaces»:«4»:«2»›");
    test_path!("    at ‹«/例👈/Cool Spaces»(«4»,«2»)›");

    // Python
    test_path!("‹«👉例wesome.py»›");
    test_path!("‹«例👈wesome.py»›");
    test_path!("    ‹File \"«/👉例wesome.py»\", line «42»›: Wat?");
    test_path!("    ‹File \"«/例👈wesome.py»\", line «42»›: Wat?");
}

#[test]
fn non_word_wide_chars() {
    // Mojo diagnostic message
    test_path!("    ‹File \"«/awe👉some.🔥»\", line «42»›: Wat?");
    test_path!("    ‹File \"«/awesome👉.🔥»\", line «42»›: Wat?");
    test_path!("    ‹File \"«/awesome.👉🔥»\", line «42»›: Wat?");
    test_path!("    ‹File \"«/awesome.🔥👈»\", line «42»›: Wat?");
}

/// These likely rise to the level of being worth fixing.
mod issues {
    #[test]
    // <https://github.com/alacritty/alacritty/issues/8586>
    fn issue_alacritty_8586() {
        // Rust paths
        test_path!("‹«/👉例/cool.rs»›");
        test_path!("‹«/例👈/cool.rs»›");
        test_path!("‹«/例/cool.rs»:«👉4»›");
        test_path!("‹«/例/cool.rs»:«4»:«👉2»›");

        // Cargo output
        test_path!("    Compiling Cool (‹«/👉例/Cool»›)");
        test_path!("    Compiling Cool (‹«/例👈/Cool»›)");

        // Python
        test_path!("‹«👉例wesome.py»›");
        test_path!("‹«例👈wesome.py»›");
        test_path!("    ‹File \"«/👉例wesome.py»\", line «42»›: Wat?");
        test_path!("    ‹File \"«/例👈wesome.py»\", line «42»›: Wat?");
    }

    #[test]
    // <https://github.com/mav-industries/mav/issues/12338>
    fn issue_12338_regex() {
        // Issue #12338
        test_path!(".rw-r--r--     0     staff 05-27 14:03 ‹«'test file 👉1.txt'»›");
        test_path!(".rw-r--r--     0     staff 05-27 14:03 ‹«👉'test file 1.txt'»›");
    }

    #[test]
    // <https://github.com/mav-industries/mav/issues/12338>
    fn issue_12338() {
        // Issue #12338
        test_path!(".rw-r--r--     0     staff 05-27 14:03 ‹«test👉、2.txt»›");
        test_path!(".rw-r--r--     0     staff 05-27 14:03 ‹«test、👈2.txt»›");
        test_path!(".rw-r--r--     0     staff 05-27 14:03 ‹«test👉。3.txt»›");
        test_path!(".rw-r--r--     0     staff 05-27 14:03 ‹«test。👈3.txt»›");

        // Rust paths
        test_path!("‹«/👉🏃/🦀.rs»›");
        test_path!("‹«/🏃👈/🦀.rs»›");
        test_path!("‹«/🏃/👉🦀.rs»:«4»›");
        test_path!("‹«/🏃/🦀👈.rs»:«4»:«2»›");

        // Cargo output
        test_path!("    Compiling Cool (‹«/👉🏃/Cool»›)");
        test_path!("    Compiling Cool (‹«/🏃👈/Cool»›)");

        // Python
        test_path!("‹«👉🏃wesome.py»›");
        test_path!("‹«🏃👈wesome.py»›");
        test_path!("    ‹File \"«/👉🏃wesome.py»\", line «42»›: Wat?");
        test_path!("    ‹File \"«/🏃👈wesome.py»\", line «42»›: Wat?");

        // Mojo
        test_path!("‹«/awe👉some.🔥»› is some good Mojo!");
        test_path!("‹«/awesome👉.🔥»› is some good Mojo!");
        test_path!("‹«/awesome.👉🔥»› is some good Mojo!");
        test_path!("‹«/awesome.🔥👈»› is some good Mojo!");
        test_path!("    ‹File \"«/👉🏃wesome.🔥»\", line «42»›: Wat?");
        test_path!("    ‹File \"«/🏃👈wesome.🔥»\", line «42»›: Wat?");
    }

    #[test]
    // <https://github.com/mav-industries/mav/issues/40202>
    fn issue_40202() {
        // Elixir
        test_path!("[‹«lib/blitz_apex_👉server/stats/aggregate_rank_stats.ex»:«35»›: BlitzApexServer.Stats.AggregateRankStats.update/2]
                1 #=> 1");
    }

    #[test]
    // <https://github.com/mav-industries/mav/issues/28194>
    fn issue_28194() {
        test_path!(
            "‹«test/c👉ontrollers/template_items_controller_test.rb»:«20»›:in 'block (2 levels) in <class:TemplateItemsControllerTest>'"
        );
    }

    #[test]
    // <https://github.com/mav-industries/mav/issues/50531>
    fn issue_50531() {
        // Paths preceded by "N:" prefix (e.g. grep output line numbers)
        // should still be clickable
        test_path!("0: ‹«foo/👉bar.txt»›");
        test_path!("0: ‹«👉foo/bar.txt»›");
        test_path!("42: ‹«👉foo/bar.txt»›");
        test_path!("1: ‹«/👉test/cool.rs»›");
        test_path!("1: ‹«/👉test/cool.rs»:«4»:«2»›");
    }

    #[test]
    // <https://github.com/mav-industries/mav/issues/46795>
    fn issue_46795() {
        // Box drawing characters are commonly used as UI elements and
        // should not interfere with path detection; they appear rarely
        // enough in actual paths that false positives should be minimal

        test_path!("─‹«/👉test/cool.rs»:«4»:«2»›");
        test_path!("┤‹«/👉test/cool.rs»:«4»:«2»›");
        test_path!("╿‹«/👉test/cool.rs»:«4»:«2»›");

        test_path!("└──‹«/👉test/cool.rs»:«4»:«2»›");
        test_path!("├─[‹«/👉test/cool.rs»:«4»:«2»›]");
        test_path!("─[‹«/👉test/cool.rs»:«4»:«2»›]");
        test_path!("┬‹«/👉test/cool.rs»:«4»:«2»›┬");
    }

    #[test]
    #[cfg_attr(
        not(target_os = "windows"),
        should_panic(expected = "Path = «/test/cool.rs:4:NotDesc», at grid cells (0, 1)..=(7, 2)")
    )]
    #[cfg_attr(
        target_os = "windows",
        should_panic(
            expected = r#"Path = «C:\\test\\cool.rs:4:NotDesc», at grid cells (0, 1)..=(8, 1)"#
        )
    )]
    // PathWithPosition::parse_str considers "/test/co👉ol.rs:4:NotDesc" invalid input, but
    // still succeeds and truncates the part after the position. Ideally this would be
    // parsed as the path "/test/co👉ol.rs:4:NotDesc" with no position.
    fn path_with_position_parse_str() {
        test_path!("`‹«/test/co👉ol.rs:4:NotDesc»›`");
        test_path!("<‹«/test/co👉ol.rs:4:NotDesc»›>");

        test_path!("'‹«(/test/co👉ol.rs:4:2)»›'");
        test_path!("'‹«(/test/co👉ol.rs(4))»›'");
        test_path!("'‹«(/test/co👉ol.rs(4,2))»›'");
    }
}

/// Minor issues arguably not important enough to fix/workaround...
mod nits {
    #[test]
    fn alacritty_bugs_with_two_columns() {
        test_path!("‹«/👉test/cool.rs»(«4»)›");
        test_path!("‹«/test/cool.rs»(«👉4»)›");
        test_path!("‹«/test/cool.rs»(«4»,«👉2»)›");

        // Python
        test_path!("‹«awe👉some.py»›");
    }

    #[test]
    #[cfg_attr(
        not(target_os = "windows"),
        should_panic(expected = "Path = «/test/cool.rs», line = 1, at grid cells (0, 0)..=(9, 0)")
    )]
    #[cfg_attr(
        target_os = "windows",
        should_panic(
            expected = r#"Path = «C:\\test\\cool.rs», line = 1, at grid cells (0, 0)..=(9, 2)"#
        )
    )]
    fn invalid_row_column_should_be_part_of_path() {
        test_path!("‹«/👉test/cool.rs:1:618033988749»›");
        test_path!("‹«/👉test/cool.rs(1,618033988749)»›");
    }

    #[test]
    #[cfg_attr(
        not(target_os = "windows"),
        should_panic(expected = "Path = «/te:st/co:ol.r:s:4:2::::::»")
    )]
    #[cfg_attr(
        target_os = "windows",
        should_panic(expected = r#"Path = «C:\\te:st\\co:ol.r:s:4:2::::::»"#)
    )]
    fn many_trailing_colons_should_be_parsed_as_part_of_the_path() {
        test_path!("‹«/te:st/👉co:ol.r:s:4:2::::::»›");
        test_path!("/test/cool.rs:::👉:");
    }

    #[test]
    // Filenames with balanced parentheses are preserved as a single path.
    // Unbalanced leading `(` (e.g. `Update(.claude/SKILL.md)`) is stripped.
    fn parens_in_filename() {
        test_path!("‹«docker-compose.prod(👉copy).yml»›");
    }
}

mod windows {
    // Lots of fun to be had with long file paths (verbatim) and UNC paths on Windows.
    // See <https://learn.microsoft.com/en-us/windows/win32/fileio/maximum-file-path-limitation>
    // See <https://users.rust-lang.org/t/understanding-windows-paths/58583>
    // See <https://github.com/rust-lang/cargo/issues/13919>

    #[test]
    fn default_prompts() {
        // Windows command prompt
        test_path!(r#"‹«C:\Users\someone\👉test»›>"#);
        test_path!(r#"C:\Users\someone\test👉>"#);

        // Windows PowerShell
        test_path!(r#"PS ‹«C:\Users\someone\👉test\cool.rs»›>"#);
        test_path!(r#"PS C:\Users\someone\test\cool.rs👉>"#);
    }

    #[test]
    fn unc() {
        test_path!(r#"‹«\\server\share\👉test\cool.rs»›"#);
        test_path!(r#"‹«\\server\share\test\cool👉.rs»›"#);
    }

    mod issues {
        #[test]
        fn issue_verbatim() {
            test_path!(r#"‹«\\?\C:\👉test\cool.rs»›"#);
            test_path!(r#"‹«\\?\C:\test\cool👉.rs»›"#);
        }

        #[test]
        fn issue_verbatim_unc() {
            test_path!(r#"‹«\\?\UNC\server\share\👉test\cool.rs»›"#);
            test_path!(r#"‹«\\?\UNC\server\share\test\cool👉.rs»›"#);
        }
    }
}
