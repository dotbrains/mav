use super::*;

pub(super) enum RedirectNormalization {
    Normalized(String),
    Skip,
}

fn is_known_safe_redirect_target(normalized_target: &str) -> bool {
    normalized_target == "/dev/null"
}

pub(super) fn normalize_io_redirect(redirect: &ast::IoRedirect) -> Option<RedirectNormalization> {
    match redirect {
        ast::IoRedirect::File(fd, kind, target) => {
            let target_word = match target {
                ast::IoFileRedirectTarget::Filename(word) => word,
                _ => return Some(RedirectNormalization::Skip),
            };
            let operator = match kind {
                ast::IoFileRedirectKind::Read => "<",
                ast::IoFileRedirectKind::Write => ">",
                ast::IoFileRedirectKind::Append => ">>",
                ast::IoFileRedirectKind::ReadAndWrite => "<>",
                ast::IoFileRedirectKind::Clobber => ">|",
                // The parser pairs DuplicateInput/DuplicateOutput with
                // IoFileRedirectTarget::Duplicate (not Filename), so the
                // target match above will return Skip before we reach here.
                // These arms are kept for defensiveness.
                ast::IoFileRedirectKind::DuplicateInput => "<&",
                ast::IoFileRedirectKind::DuplicateOutput => ">&",
            };
            let fd_prefix = match fd {
                Some(fd) => fd.to_string(),
                None => String::new(),
            };
            let normalized = normalize_word(target_word)?;
            if is_known_safe_redirect_target(&normalized) {
                return Some(RedirectNormalization::Skip);
            }
            Some(RedirectNormalization::Normalized(format!(
                "{}{} {}",
                fd_prefix, operator, normalized
            )))
        }
        ast::IoRedirect::OutputAndError(word, append) => {
            let operator = if *append { "&>>" } else { "&>" };
            let normalized = normalize_word(word)?;
            if is_known_safe_redirect_target(&normalized) {
                return Some(RedirectNormalization::Skip);
            }
            Some(RedirectNormalization::Normalized(format!(
                "{} {}",
                operator, normalized
            )))
        }
        ast::IoRedirect::HereDocument(_, _) | ast::IoRedirect::HereString(_, _) => {
            Some(RedirectNormalization::Skip)
        }
    }
}
