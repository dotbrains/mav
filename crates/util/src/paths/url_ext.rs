use super::*;

pub trait UrlExt {
    /// A version of `url::Url::to_file_path` that does platform handling based on the provided `PathStyle` instead of the host platform.
    ///
    /// Prefer using this over `url::Url::to_file_path` when you need to handle paths in a cross-platform way as is the case for remoting interactions.
    fn to_file_path_ext(&self, path_style: PathStyle) -> Result<PathBuf, ()>;
}

impl UrlExt for url::Url {
    // Copied from `url::Url::to_file_path`, but the `cfg` handling is replaced with runtime branching on `PathStyle`
    fn to_file_path_ext(&self, source_path_style: PathStyle) -> Result<PathBuf, ()> {
        if let Some(segments) = self.path_segments() {
            let host = match self.host() {
                None | Some(url::Host::Domain("localhost")) => None,
                Some(_) if source_path_style.is_windows() && self.scheme() == "file" => {
                    self.host_str()
                }
                _ => return Err(()),
            };

            let str_len = self.as_str().len();
            let estimated_capacity = if source_path_style.is_windows() {
                // remove scheme: - has possible \\ for hostname
                str_len.saturating_sub(self.scheme().len() + 1)
            } else {
                // remove scheme://
                str_len.saturating_sub(self.scheme().len() + 3)
            };
            return match source_path_style {
                PathStyle::Posix => {
                    file_url_segments_to_pathbuf_posix(estimated_capacity, host, segments)
                }
                PathStyle::Windows => {
                    file_url_segments_to_pathbuf_windows(estimated_capacity, host, segments)
                }
            };
        }

        fn file_url_segments_to_pathbuf_posix(
            estimated_capacity: usize,
            host: Option<&str>,
            segments: std::str::Split<'_, char>,
        ) -> Result<PathBuf, ()> {
            use percent_encoding::percent_decode;

            if host.is_some() {
                return Err(());
            }

            let mut bytes = Vec::new();
            bytes.try_reserve(estimated_capacity).map_err(|_| ())?;

            for segment in segments {
                bytes.push(b'/');
                bytes.extend(percent_decode(segment.as_bytes()));
            }

            // A windows drive letter must end with a slash.
            if bytes.len() > 2
                && bytes[bytes.len() - 2].is_ascii_alphabetic()
                && matches!(bytes[bytes.len() - 1], b':' | b'|')
            {
                bytes.push(b'/');
            }

            let path = String::from_utf8(bytes).map_err(|_| ())?;
            debug_assert!(
                PathStyle::Posix.is_absolute(&path),
                "to_file_path() failed to produce an absolute Path"
            );

            Ok(PathBuf::from(path))
        }

        fn file_url_segments_to_pathbuf_windows(
            estimated_capacity: usize,
            host: Option<&str>,
            mut segments: std::str::Split<'_, char>,
        ) -> Result<PathBuf, ()> {
            use percent_encoding::percent_decode_str;
            let mut string = String::new();
            string.try_reserve(estimated_capacity).map_err(|_| ())?;
            if let Some(host) = host {
                string.push_str(r"\\");
                string.push_str(host);
            } else {
                let first = segments.next().ok_or(())?;

                match first.len() {
                    2 => {
                        if !first.starts_with(|c| char::is_ascii_alphabetic(&c))
                            || first.as_bytes()[1] != b':'
                        {
                            return Err(());
                        }

                        string.push_str(first);
                    }

                    4 => {
                        if !first.starts_with(|c| char::is_ascii_alphabetic(&c)) {
                            return Err(());
                        }
                        let bytes = first.as_bytes();
                        if bytes[1] != b'%'
                            || bytes[2] != b'3'
                            || (bytes[3] != b'a' && bytes[3] != b'A')
                        {
                            return Err(());
                        }

                        string.push_str(&first[0..1]);
                        string.push(':');
                    }

                    _ => return Err(()),
                }
            };

            for segment in segments {
                string.push('\\');

                // Currently non-unicode windows paths cannot be represented
                match percent_decode_str(segment).decode_utf8() {
                    Ok(s) => string.push_str(&s),
                    Err(..) => return Err(()),
                }
            }
            // ensure our estimated capacity was good
            if cfg!(test) {
                debug_assert!(
                    string.len() <= estimated_capacity,
                    "len: {}, capacity: {}",
                    string.len(),
                    estimated_capacity
                );
            }
            debug_assert!(
                PathStyle::Windows.is_absolute(&string),
                "to_file_path() failed to produce an absolute Path"
            );
            let path = PathBuf::from(string);
            Ok(path)
        }
        Err(())
    }
}
