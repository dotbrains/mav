use super::*;

/// reproduction of issue #50785
fn build_pcm16_wav_bytes() -> Vec<u8> {
    let header: Vec<u8> = vec![
        /*  RIFF header  */
        0x52, 0x49, 0x46, 0x46, // "RIFF"
        0xc6, 0xcf, 0x00, 0x00, // file size: 8
        0x57, 0x41, 0x56, 0x45, // "WAVE"
        /*  fmt chunk  */
        0x66, 0x6d, 0x74, 0x20, // "fmt "
        0x10, 0x00, 0x00, 0x00, // chunk size: 16
        0x01, 0x00, // format: PCM (1)
        0x01, 0x00, // channels: 1 (mono)
        0x80, 0x3e, 0x00, 0x00, // sample rate: 16000
        0x00, 0x7d, 0x00, 0x00, // byte rate: 32000
        0x02, 0x00, // block align: 2
        0x10, 0x00, // bits per sample: 16
        /*  LIST chunk  */
        0x4c, 0x49, 0x53, 0x54, // "LIST"
        0x1a, 0x00, 0x00, 0x00, // chunk size: 26
        0x49, 0x4e, 0x46, 0x4f, // "INFO"
        0x49, 0x53, 0x46, 0x54, // "ISFT"
        0x0d, 0x00, 0x00, 0x00, // sub-chunk size: 13
        0x4c, 0x61, 0x76, 0x66, 0x36, 0x32, 0x2e, 0x33, // "Lavf62.3"
        0x2e, 0x31, 0x30, 0x30, 0x00, // ".100\0"
        /* padding byte for word alignment */
        0x00, // data chunk header
        0x64, 0x61, 0x74, 0x61, // "data"
        0x80, 0xcf, 0x00, 0x00, // chunk size
    ];

    let mut bytes = header;

    // fill remaining space up to `FILE_ANALYSIS_BYTES` with synthetic PCM
    let audio_bytes_needed = FILE_ANALYSIS_BYTES - bytes.len();
    for i in 0..(audio_bytes_needed / 2) {
        let sample = (i & 0xFF) as u8;
        bytes.push(sample); // low byte: varies
        bytes.push(0x00); // high byte: zero for small values
    }

    bytes
}

#[test]
fn test_pcm16_wav_detected_as_binary() {
    let wav_bytes = build_pcm16_wav_bytes();
    assert_eq!(wav_bytes.len(), FILE_ANALYSIS_BYTES);

    let result = analyze_byte_content(&wav_bytes);
    assert_eq!(
        result,
        ByteContent::Binary,
        "PCM 16-bit WAV should be detected as Binary via RIFF header"
    );
}

#[test]
fn test_le16_binary_not_misdetected_as_utf16le() {
    let mut bytes = b"FAKE".to_vec();
    while bytes.len() < FILE_ANALYSIS_BYTES {
        let sample = (bytes.len() & 0xFF) as u8;
        bytes.push(sample);
        bytes.push(0x00);
    }
    bytes.truncate(FILE_ANALYSIS_BYTES);

    let result = analyze_byte_content(&bytes);
    assert_eq!(
        result,
        ByteContent::Binary,
        "LE 16-bit binary with control characters should be detected as Binary"
    );
}

#[test]
fn test_be16_binary_not_misdetected_as_utf16be() {
    let mut bytes = b"FAKE".to_vec();
    while bytes.len() < FILE_ANALYSIS_BYTES {
        bytes.push(0x00);
        let sample = (bytes.len() & 0xFF) as u8;
        bytes.push(sample);
    }
    bytes.truncate(FILE_ANALYSIS_BYTES);

    let result = analyze_byte_content(&bytes);
    assert_eq!(
        result,
        ByteContent::Binary,
        "BE 16-bit binary with control characters should be detected as Binary"
    );
}

#[test]
fn test_utf16le_text_detected_as_utf16le() {
    let text = "Hello, world! This is a UTF-16 test string. ";
    let mut bytes = Vec::new();
    while bytes.len() < FILE_ANALYSIS_BYTES {
        bytes.extend(text.encode_utf16().flat_map(|u| u.to_le_bytes()));
    }
    bytes.truncate(FILE_ANALYSIS_BYTES);

    assert_eq!(analyze_byte_content(&bytes), ByteContent::Utf16Le);
}

#[test]
fn test_utf16be_text_detected_as_utf16be() {
    let text = "Hello, world! This is a UTF-16 test string. ";
    let mut bytes = Vec::new();
    while bytes.len() < FILE_ANALYSIS_BYTES {
        bytes.extend(text.encode_utf16().flat_map(|u| u.to_be_bytes()));
    }
    bytes.truncate(FILE_ANALYSIS_BYTES);

    assert_eq!(analyze_byte_content(&bytes), ByteContent::Utf16Be);
}

#[test]
fn test_known_binary_headers() {
    let cases: &[(&[u8], &str)] = &[
        (b"RIFF\x00\x00\x00\x00WAVE", "WAV"),
        (b"RIFF\x00\x00\x00\x00AVI ", "AVI"),
        (b"OggS\x00\x02", "OGG"),
        (b"fLaC\x00\x00", "FLAC"),
        (b"ID3\x03\x00", "MP3 ID3v2"),
        (b"\xFF\xFB\x90\x00", "MP3 MPEG1 Layer3"),
        (b"\xFF\xF3\x90\x00", "MP3 MPEG2 Layer3"),
    ];

    for (header, label) in cases {
        let mut bytes = header.to_vec();
        bytes.resize(FILE_ANALYSIS_BYTES, 0x41); // pad with 'A'
        assert_eq!(
            analyze_byte_content(&bytes),
            ByteContent::Binary,
            "{label} should be detected as Binary"
        );
    }
}
