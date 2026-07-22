use super::*;

impl<'a> From<&'a str> for Rope {
    fn from(text: &'a str) -> Self {
        let mut rope = Self::new();
        rope.push(text);
        rope
    }
}

impl<'a> FromIterator<&'a str> for Rope {
    fn from_iter<T: IntoIterator<Item = &'a str>>(iter: T) -> Self {
        let mut rope = Rope::new();
        for chunk in iter {
            rope.push(chunk);
        }
        rope
    }
}

impl From<String> for Rope {
    #[inline(always)]
    fn from(text: String) -> Self {
        Rope::from(text.as_str())
    }
}

impl From<&String> for Rope {
    #[inline(always)]
    fn from(text: &String) -> Self {
        Rope::from(text.as_str())
    }
}

impl fmt::Display for Rope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for chunk in self.chunks() {
            write!(f, "{}", chunk)?;
        }
        Ok(())
    }
}

impl fmt::Debug for Rope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use std::fmt::Write as _;

        write!(f, "\"")?;
        let mut format_string = String::new();
        for chunk in self.chunks() {
            write!(&mut format_string, "{:?}", chunk)?;
            write!(f, "{}", &format_string[1..format_string.len() - 1])?;
            format_string.clear();
        }
        write!(f, "\"")?;
        Ok(())
    }
}
