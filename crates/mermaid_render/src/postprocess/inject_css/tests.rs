use super::*;

#[test]
fn scope_css_prefixes_selectors() {
    let input = "        .foo { color: red; }\n";
    let result = scope_css(input, "my-svg");
    assert!(result.contains("#my-svg .foo"), "got: {result}");
}
