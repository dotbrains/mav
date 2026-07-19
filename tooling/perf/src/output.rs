use crate::{
    QUIET,
    implementation::{Output, consts},
};
use std::{
    fs::OpenOptions,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::Ordering,
};

/// How does this perf run return its output?
pub(crate) enum OutputKind<'a> {
    /// Print markdown to the terminal.
    Markdown,
    /// Save JSON to a file.
    Json(&'a Path),
}

impl OutputKind<'_> {
    /// Logs the output of a run as per the `OutputKind`.
    pub(crate) fn log(&self, output: &Output, t_bin: &str) {
        match self {
            OutputKind::Markdown => println!("{output}"),
            OutputKind::Json(ident) => {
                let runs_dir = workspace_dir().join(consts::RUNS_DIR);
                std::fs::create_dir_all(&runs_dir).unwrap();
                assert!(
                    !ident.to_string_lossy().is_empty(),
                    "FATAL: Empty filename specified!"
                );
                // Get the test binary's crate's name; a path like
                // target/release-fast/deps/gpui-061ff76c9b7af5d7
                // would be reduced to just "gpui".
                let test_bin_stripped = Path::new(t_bin)
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .rsplit_once('-')
                    .unwrap()
                    .0;
                let mut file_path = runs_dir.join(ident);
                file_path
                    .as_mut_os_string()
                    .push(format!(".{test_bin_stripped}.json"));
                let mut out_file = OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .open(&file_path)
                    .unwrap();
                out_file
                    .write_all(&serde_json::to_vec(&output).unwrap())
                    .unwrap();
                if !QUIET.load(Ordering::Relaxed) {
                    eprintln!("JSON output written to {}", file_path.display());
                }
            }
        }
    }
}

/// Compares the perf results of two profiles as per the arguments passed in.
pub(crate) fn compare_profiles(args: &[String]) {
    let mut save_to = None;
    let mut ident_idx = 0;
    args.first().inspect(|a| {
        if a.starts_with("--save") {
            save_to = Some(
                a.strip_prefix("--save=")
                    .expect("FATAL: save param formatted incorrectly"),
            );
            ident_idx = 1;
        }
    });
    let ident_new = args
        .get(ident_idx)
        .expect("FATAL: missing identifier for new run");
    let ident_old = args
        .get(ident_idx + 1)
        .expect("FATAL: missing identifier for old run");

    let runs_dir = workspace_dir().join(consts::RUNS_DIR);
    let mut outputs_new = Output::blank();
    let mut outputs_old = Output::blank();

    for e in runs_dir.read_dir().unwrap() {
        let Ok(entry) = e else {
            continue;
        };
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if metadata.is_file() {
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            let read_into = |output: &mut Output| {
                let mut elems = name.split('.').skip(1);
                let prefix = elems.next().unwrap();
                assert_eq!("json", elems.next().unwrap());
                assert!(elems.next().is_none());
                let mut buffer = Vec::new();
                let _ = OpenOptions::new()
                    .read(true)
                    .open(entry.path())
                    .unwrap()
                    .read_to_end(&mut buffer)
                    .unwrap();
                let o_other: Output = serde_json::from_slice(&buffer).unwrap();
                output.merge(o_other, prefix);
            };

            if name.starts_with(ident_old) {
                read_into(&mut outputs_old);
            } else if name.starts_with(ident_new) {
                read_into(&mut outputs_new);
            }
        }
    }

    let res = outputs_new.compare_perf(outputs_old);
    if let Some(filename) = save_to {
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(filename)
            .expect("FATAL: couldn't save run results to file");
        file.write_all(format!("{res}").as_bytes()).unwrap();
    } else {
        println!("{res}");
    }
}

fn workspace_dir() -> PathBuf {
    PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("..")
        .join("..")
}
