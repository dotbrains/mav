use super::*;

pub(super) async fn decode_file_text(
    fs: &dyn Fs,
    abs_path: &Path,
) -> Result<(String, &'static Encoding, bool)> {
    let mut file = fs
        .open_sync(&abs_path)
        .await
        .with_context(|| format!("opening file {abs_path:?}"))?;

    // First, read the beginning of the file to determine its kind and encoding.
    // We do not want to load an entire large blob into memory only to discard it.
    let mut file_first_bytes = Vec::with_capacity(FILE_ANALYSIS_BYTES);
    let mut buf = [0u8; FILE_ANALYSIS_BYTES];
    let mut reached_eof = false;
    loop {
        if file_first_bytes.len() >= FILE_ANALYSIS_BYTES {
            break;
        }
        let n = file
            .read(&mut buf)
            .with_context(|| format!("reading bytes of the file {abs_path:?}"))?;
        if n == 0 {
            reached_eof = true;
            break;
        }
        file_first_bytes.extend_from_slice(&buf[..n]);
    }
    let (bom_encoding, byte_content) = decode_byte_header(&file_first_bytes);
    anyhow::ensure!(
        byte_content != ByteContent::Binary,
        "Binary files are not supported"
    );

    // If the file is eligible for opening, read the rest of the file.
    let mut content = file_first_bytes;
    if !reached_eof {
        let mut buf = [0u8; 8 * 1024];
        loop {
            let n = file
                .read(&mut buf)
                .with_context(|| format!("reading remaining bytes of the file {abs_path:?}"))?;
            if n == 0 {
                break;
            }
            content.extend_from_slice(&buf[..n]);
        }
    }
    decode_byte_full(content, bom_encoding, byte_content)
}

fn decode_byte_header(prefix: &[u8]) -> (Option<&'static Encoding>, ByteContent) {
    if let Some((encoding, _bom_len)) = Encoding::for_bom(prefix) {
        return (Some(encoding), ByteContent::Unknown);
    }
    (None, analyze_byte_content(prefix))
}

fn decode_byte_full(
    bytes: Vec<u8>,
    bom_encoding: Option<&'static Encoding>,
    byte_content: ByteContent,
) -> Result<(String, &'static Encoding, bool)> {
    if let Some(encoding) = bom_encoding {
        let (cow, _) = encoding.decode_with_bom_removal(&bytes);
        return Ok((cow.into_owned(), encoding, true));
    }

    match byte_content {
        ByteContent::Utf16Le => {
            let encoding = encoding_rs::UTF_16LE;
            let (cow, _, _) = encoding.decode(&bytes);
            return Ok((cow.into_owned(), encoding, false));
        }
        ByteContent::Utf16Be => {
            let encoding = encoding_rs::UTF_16BE;
            let (cow, _, _) = encoding.decode(&bytes);
            return Ok((cow.into_owned(), encoding, false));
        }
        ByteContent::Binary => {
            anyhow::bail!("Binary files are not supported");
        }
        ByteContent::Unknown => {}
    }

    fn detect_encoding(bytes: Vec<u8>) -> (String, &'static Encoding) {
        let mut detector = EncodingDetector::new();
        detector.feed(&bytes, true);

        let encoding = detector.guess(None, true); // Use None for TLD hint to ensure neutral detection logic.

        let (cow, _, _) = encoding.decode(&bytes);
        (cow.into_owned(), encoding)
    }

    match String::from_utf8(bytes) {
        Ok(text) => {
            // ISO-2022-JP (and other ISO-2022 variants) consists entirely of 7-bit ASCII bytes,
            // so it is valid UTF-8. However, it contains escape sequences starting with '\x1b'.
            // If we find an escape character, we double-check the encoding to prevent
            // displaying raw escape sequences instead of the correct characters.
            if text.contains('\x1b') {
                let (s, enc) = detect_encoding(text.into_bytes());
                Ok((s, enc, false))
            } else {
                Ok((text, encoding_rs::UTF_8, false))
            }
        }
        Err(e) => {
            let (s, enc) = detect_encoding(e.into_bytes());
            Ok((s, enc, false))
        }
    }
}
