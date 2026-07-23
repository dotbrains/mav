/// [**`c₀, c₁, …, cₙ;`**]ₒₚₜ := use specified terminal widths of `c₀, c₁, …, cₙ` **columns**
/// (defaults to `3, longest_line_cells / 2, longest_line_cells + 1;`)
///
macro_rules! test_iri {
            ($iri:literal) => { { test_hyperlink!(concat!("‹«👉", $iri, "»›"); Iri) } };
        }

#[test]
fn simple() {
    // In the order they appear in URL_REGEX, except 'file://' which is treated as a path
    test_iri!("ipfs://test/cool.ipfs");
    test_iri!("ipns://test/cool.ipns");
    test_iri!("magnet://test/cool.git");
    test_iri!("mailto:someone@somewhere.here");
    test_iri!("gemini://somewhere.here");
    test_iri!("gopher://somewhere.here");
    test_iri!("http://test/cool/index.html");
    test_iri!("http://10.10.10.10:1111/cool.html");
    test_iri!("http://test/cool/index.html?amazing=1");
    test_iri!("http://test/cool/index.html#right%20here");
    test_iri!("http://test/cool/index.html?amazing=1#right%20here");
    test_iri!("https://test/cool/index.html");
    test_iri!("https://10.10.10.10:1111/cool.html");
    test_iri!("https://test/cool/index.html?amazing=1");
    test_iri!("https://test/cool/index.html#right%20here");
    test_iri!("https://test/cool/index.html?amazing=1#right%20here");
    test_iri!("news://test/cool.news");
    test_iri!("git://test/cool.git");
    test_iri!("ssh://user@somewhere.over.here:12345/test/cool.git");
    test_iri!("ftp://test/cool.ftp");
}

#[test]
fn wide_chars() {
    // In the order they appear in URL_REGEX, except 'file://' which is treated as a path
    test_iri!("ipfs://例🏃🦀/cool.ipfs");
    test_iri!("ipns://例🏃🦀/cool.ipns");
    test_iri!("magnet://例🏃🦀/cool.git");
    test_iri!("mailto:someone@somewhere.here");
    test_iri!("gemini://somewhere.here");
    test_iri!("gopher://somewhere.here");
    test_iri!("http://例🏃🦀/cool/index.html");
    test_iri!("http://10.10.10.10:1111/cool.html");
    test_iri!("http://例🏃🦀/cool/index.html?amazing=1");
    test_iri!("http://例🏃🦀/cool/index.html#right%20here");
    test_iri!("http://例🏃🦀/cool/index.html?amazing=1#right%20here");
    test_iri!("https://例🏃🦀/cool/index.html");
    test_iri!("https://10.10.10.10:1111/cool.html");
    test_iri!("https://例🏃🦀/cool/index.html?amazing=1");
    test_iri!("https://例🏃🦀/cool/index.html#right%20here");
    test_iri!("https://例🏃🦀/cool/index.html?amazing=1#right%20here");
    test_iri!("news://例🏃🦀/cool.news");
    test_iri!("git://例/cool.git");
    test_iri!("ssh://user@somewhere.over.here:12345/例🏃🦀/cool.git");
    test_iri!("ftp://例🏃🦀/cool.ftp");
}

// There are likely more tests needed for IRI vs URI
#[test]
fn iris() {
    // These refer to the same location, see example here:
    // <https://en.wikipedia.org/wiki/Internationalized_Resource_Identifier#Compatibility>
    test_iri!("https://en.wiktionary.org/wiki/Ῥόδος"); // IRI
    test_iri!("https://en.wiktionary.org/wiki/%E1%BF%AC%CF%8C%CE%B4%CE%BF%CF%82"); // URI
}

#[test]
#[should_panic(expected = "Expected a path, but was a iri")]
fn file_is_a_path() {
    test_iri!("file://test/cool/index.rs");
}
