use super::*;

#[derive(Clone, Debug, PartialEq)]
pub struct Replacement {
    pub(super) search: String,
    pub(super) replacement: String,
    pub(super) case_sensitive: Option<bool>,
    pub(super) flag_n: bool,
    pub(super) flag_g: bool,
    pub(super) flag_c: bool,
}

impl Replacement {
    // convert a vim query into something more usable by mav.
    // we don't attempt to fully convert between the two regex syntaxes,
    // but we do flip \( and \) to ( and ) (and vice-versa) in the pattern,
    // convert \0..\9 to $0..$9 in the replacement so that common idioms work,
    // and escape literal `$` to `$$` in the replacement so vim's literal `$`
    // is not interpreted as a Rust regex capture-group reference.
    pub(crate) fn parse(mut chars: Peekable<Chars>) -> Option<Replacement> {
        let delimiter = chars
            .next()
            .filter(|c| !c.is_alphanumeric() && *c != '"' && *c != '|' && *c != '\'')?;

        let mut search = String::new();
        let mut replacement = String::new();
        let mut flags = String::new();

        let mut buffer = &mut search;

        let mut escaped = false;
        // 0 - parsing search
        // 1 - parsing replacement
        // 2 - parsing flags
        let mut phase = 0;

        for c in chars {
            if escaped {
                escaped = false;
                if phase == 1 && c.is_ascii_digit() {
                    buffer.push('$')
                } else if phase == 1 && c == '$' {
                    // Second '$' escapes by fallthrough
                    buffer.push('$')
                // unescape escaped parens
                } else if phase == 0 && (c == '(' || c == ')') {
                } else if c != delimiter {
                    buffer.push('\\')
                }
                buffer.push(c)
            } else if c == '\\' {
                escaped = true;
            } else if c == delimiter {
                if phase == 0 {
                    buffer = &mut replacement;
                    phase = 1;
                } else if phase == 1 {
                    buffer = &mut flags;
                    phase = 2;
                } else {
                    break;
                }
            } else {
                // escape unescaped parens
                if phase == 0 && (c == '(' || c == ')') {
                    buffer.push('\\')
                } else if phase == 1 && c == '$' {
                    // '$' is not special in the replacement clause,
                    // so we also escape here.
                    buffer.push('$')
                }
                buffer.push(c)
            }
        }

        let mut replacement = Replacement {
            search,
            replacement,
            case_sensitive: None,
            flag_g: false,
            flag_n: false,
            flag_c: false,
        };

        for c in flags.chars() {
            match c {
                'g' => replacement.flag_g = !replacement.flag_g,
                'n' => replacement.flag_n = true,
                'c' => replacement.flag_c = true,
                'i' => replacement.case_sensitive = Some(false),
                'I' => replacement.case_sensitive = Some(true),
                _ => {}
            }
        }

        Some(replacement)
    }
}
