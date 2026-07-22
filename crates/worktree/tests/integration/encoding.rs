use super::*;

#[gpui::test]
async fn test_load_file_encoding(cx: &mut TestAppContext) {
    init_test(cx);

    struct TestCase {
        name: &'static str,
        bytes: Vec<u8>,
        expected_text: &'static str,
    }

    // --- Success Cases ---
    let success_cases = vec![
        TestCase {
            name: "utf8.txt",
            bytes: "こんにちは".as_bytes().to_vec(),
            expected_text: "こんにちは",
        },
        TestCase {
            name: "sjis.txt",
            bytes: vec![0x82, 0xb1, 0x82, 0xf1, 0x82, 0xc9, 0x82, 0xbf, 0x82, 0xcd],
            expected_text: "こんにちは",
        },
        TestCase {
            name: "eucjp.txt",
            bytes: vec![0xa4, 0xb3, 0xa4, 0xf3, 0xa4, 0xcb, 0xa4, 0xc1, 0xa4, 0xcf],
            expected_text: "こんにちは",
        },
        TestCase {
            name: "iso2022jp.txt",
            bytes: vec![
                0x1b, 0x24, 0x42, 0x24, 0x33, 0x24, 0x73, 0x24, 0x4b, 0x24, 0x41, 0x24, 0x4f, 0x1b,
                0x28, 0x42,
            ],
            expected_text: "こんにちは",
        },
        TestCase {
            name: "win1252.txt",
            bytes: vec![0x43, 0x61, 0x66, 0xe9],
            expected_text: "Café",
        },
        TestCase {
            name: "gbk.txt",
            bytes: vec![
                0xbd, 0xf1, 0xcc, 0xec, 0xcc, 0xec, 0xc6, 0xf8, 0xb2, 0xbb, 0xb4, 0xed,
            ],
            expected_text: "今天天气不错",
        },
        // UTF-16LE with BOM
        TestCase {
            name: "utf16le_bom.txt",
            bytes: vec![
                0xFF, 0xFE, // BOM
                0x53, 0x30, 0x93, 0x30, 0x6B, 0x30, 0x61, 0x30, 0x6F, 0x30,
            ],
            expected_text: "こんにちは",
        },
        // UTF-16BE with BOM
        TestCase {
            name: "utf16be_bom.txt",
            bytes: vec![
                0xFE, 0xFF, // BOM
                0x30, 0x53, 0x30, 0x93, 0x30, 0x6B, 0x30, 0x61, 0x30, 0x6F,
            ],
            expected_text: "こんにちは",
        },
        // UTF-16LE without BOM (ASCII only)
        // This relies on the "null byte heuristic" we implemented.
        // "ABC" -> 41 00 42 00 43 00
        TestCase {
            name: "utf16le_ascii_no_bom.txt",
            bytes: vec![0x41, 0x00, 0x42, 0x00, 0x43, 0x00],
            expected_text: "ABC",
        },
    ];

    // --- Failure Cases ---
    let failure_cases = vec![
        // Binary File (Should be detected by heuristic and return Error)
        // Contains random bytes and mixed nulls that don't match UTF-16 patterns
        TestCase {
            name: "binary.bin",
            bytes: vec![0x00, 0xFF, 0x12, 0x00, 0x99, 0x88, 0x77, 0x66, 0x00],
            expected_text: "", // Not used
        },
    ];

    let root_path = if cfg!(windows) {
        Path::new("C:\\root")
    } else {
        Path::new("/root")
    };

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.create_dir(root_path).await.unwrap();

    for case in success_cases.iter().chain(failure_cases.iter()) {
        let path = root_path.join(case.name);
        fs.write(&path, &case.bytes).await.unwrap();
    }

    let tree = Worktree::local(
        root_path,
        true,
        fs,
        Default::default(),
        true,
        WorktreeId::from_proto(0),
        &mut cx.to_async(),
    )
    .await
    .unwrap();

    cx.read(|cx| tree.read(cx).as_local().unwrap().scan_complete())
        .await;

    let rel_path = |name: &str| {
        RelPath::new(&Path::new(name), PathStyle::local())
            .unwrap()
            .into_arc()
    };

    // Run Success Tests
    for case in success_cases {
        let loaded = tree
            .update(cx, |tree, cx| tree.load_file(&rel_path(case.name), cx))
            .await;
        if let Err(e) = &loaded {
            panic!("Failed to load success case '{}': {:?}", case.name, e);
        }
        let loaded = loaded.unwrap();
        assert_eq!(
            loaded.text, case.expected_text,
            "Encoding mismatch for file: {}",
            case.name
        );
    }

    // Run Failure Tests
    for case in failure_cases {
        let loaded = tree
            .update(cx, |tree, cx| tree.load_file(&rel_path(case.name), cx))
            .await;
        assert!(
            loaded.is_err(),
            "Failure case '{}' unexpectedly succeeded! It should have been detected as binary.",
            case.name
        );
        let err_msg = loaded.unwrap_err().to_string();
        println!("Got expected error for {}: {}", case.name, err_msg);
    }
}

#[gpui::test]
async fn test_write_file_encoding(cx: &mut gpui::TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let root_path = if cfg!(windows) {
        Path::new("C:\\root")
    } else {
        Path::new("/root")
    };
    fs.create_dir(root_path).await.unwrap();

    let worktree = Worktree::local(
        root_path,
        true,
        fs.clone(),
        Default::default(),
        true,
        WorktreeId::from_proto(0),
        &mut cx.to_async(),
    )
    .await
    .unwrap();

    // Define test case structure
    struct TestCase {
        name: &'static str,
        text: &'static str,
        encoding: &'static encoding_rs::Encoding,
        has_bom: bool,
        expected_bytes: Vec<u8>,
    }

    let cases = vec![
        // Shift_JIS with Japanese
        TestCase {
            name: "Shift_JIS with Japanese",
            text: "こんにちは",
            encoding: encoding_rs::SHIFT_JIS,
            has_bom: false,
            expected_bytes: vec![0x82, 0xb1, 0x82, 0xf1, 0x82, 0xc9, 0x82, 0xbf, 0x82, 0xcd],
        },
        // UTF-8 No BOM
        TestCase {
            name: "UTF-8 No BOM",
            text: "AB",
            encoding: encoding_rs::UTF_8,
            has_bom: false,
            expected_bytes: vec![0x41, 0x42],
        },
        // UTF-8 with BOM
        TestCase {
            name: "UTF-8 with BOM",
            text: "AB",
            encoding: encoding_rs::UTF_8,
            has_bom: true,
            expected_bytes: vec![0xEF, 0xBB, 0xBF, 0x41, 0x42],
        },
        // UTF-16LE No BOM with Japanese
        // NOTE: This passes thanks to the manual encoding fix implemented in `write_file`.
        TestCase {
            name: "UTF-16LE No BOM with Japanese",
            text: "こんにちは",
            encoding: encoding_rs::UTF_16LE,
            has_bom: false,
            expected_bytes: vec![0x53, 0x30, 0x93, 0x30, 0x6b, 0x30, 0x61, 0x30, 0x6f, 0x30],
        },
        // UTF-16LE with BOM
        TestCase {
            name: "UTF-16LE with BOM",
            text: "A",
            encoding: encoding_rs::UTF_16LE,
            has_bom: true,
            expected_bytes: vec![0xFF, 0xFE, 0x41, 0x00],
        },
        // UTF-16BE No BOM with Japanese
        // NOTE: This passes thanks to the manual encoding fix.
        TestCase {
            name: "UTF-16BE No BOM with Japanese",
            text: "こんにちは",
            encoding: encoding_rs::UTF_16BE,
            has_bom: false,
            expected_bytes: vec![0x30, 0x53, 0x30, 0x93, 0x30, 0x6b, 0x30, 0x61, 0x30, 0x6f],
        },
        // UTF-16BE with BOM
        TestCase {
            name: "UTF-16BE with BOM",
            text: "A",
            encoding: encoding_rs::UTF_16BE,
            has_bom: true,
            expected_bytes: vec![0xFE, 0xFF, 0x00, 0x41],
        },
    ];

    for (i, case) in cases.into_iter().enumerate() {
        let file_name = format!("test_{}.txt", i);
        let path: Arc<Path> = Path::new(&file_name).into();
        let file_path = root_path.join(&file_name);

        fs.insert_file(&file_path, "".into()).await;

        let rel_path = RelPath::new(&path, PathStyle::local()).unwrap().into_arc();
        let text = text::Rope::from(case.text);

        let task = worktree.update(cx, |wt, cx| {
            wt.write_file(
                rel_path,
                text,
                text::LineEnding::Unix,
                case.encoding,
                case.has_bom,
                cx,
            )
        });

        if let Err(e) = task.await {
            panic!("Unexpected error in case '{}': {:?}", case.name, e);
        }

        let bytes = fs.load_bytes(&file_path).await.unwrap();

        assert_eq!(
            bytes, case.expected_bytes,
            "case '{}' mismatch. Expected {:?}, but got {:?}",
            case.name, case.expected_bytes, bytes
        );
    }
}
