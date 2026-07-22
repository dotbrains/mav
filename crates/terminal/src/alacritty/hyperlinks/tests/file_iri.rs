// File IRIs have a ton of use cases. Absolute file URIs are supported on all platforms,
// including Windows drive letters (e.g., file:///C:/path) and percent-encoded characters.
// Some cases like relative file IRIs are not supported.
// See https://en.wikipedia.org/wiki/File_URI_scheme

/// [**`c₀, c₁, …, cₙ;`**]ₒₚₜ := use specified terminal widths of `c₀, c₁, …, cₙ` **columns**
/// (defaults to `3, longest_line_cells / 2, longest_line_cells + 1;`)
///
macro_rules! test_file_iri {
            ($file_iri:literal) => { { test_hyperlink!(concat!("‹«👉", $file_iri, "»›"); FileIri) } };
        }

#[cfg(not(target_os = "windows"))]
#[test]
fn absolute_file_iri() {
    test_file_iri!("file:///test/cool/index.rs");
    test_file_iri!("file:///test/cool/");
}

mod issues {
    #[cfg(not(target_os = "windows"))]
    #[test]
    fn issue_file_iri_with_percent_encoded_characters() {
        // Non-space characters
        // file:///test/Ῥόδος/
        test_file_iri!("file:///test/%E1%BF%AC%CF%8C%CE%B4%CE%BF%CF%82/"); // URI

        // Spaces
        test_file_iri!("file:///te%20st/co%20ol/index.rs");
        test_file_iri!("file:///te%20st/co%20ol/");
    }
}

#[cfg(target_os = "windows")]
mod windows {
    mod issues {
        // The test uses Url::to_file_path(), but it seems that the Url crate doesn't
        // support relative file IRIs.
        #[test]
        #[should_panic(
            expected = r#"Failed to interpret file IRI `file:/test/cool/index.rs` as a path"#
        )]
        fn issue_relative_file_iri() {
            test_file_iri!("file:/test/cool/index.rs");
            test_file_iri!("file:/test/cool/");
        }

        // See https://en.wikipedia.org/wiki/File_URI_scheme
        // https://github.com/mav-industries/mav/issues/39189
        #[test]
        fn issue_39189() {
            test_file_iri!("file:///C:/test/cool/index.rs");
            test_file_iri!("file:///C:/test/cool/");
        }

        #[test]
        fn issue_file_iri_with_percent_encoded_characters() {
            // Non-space characters
            // file:///test/Ῥόδος/
            test_file_iri!("file:///C:/test/%E1%BF%AC%CF%8C%CE%B4%CE%BF%CF%82/"); // URI

            // Spaces
            test_file_iri!("file:///C:/te%20st/co%20ol/index.rs");
            test_file_iri!("file:///C:/te%20st/co%20ol/");
        }
    }
}
