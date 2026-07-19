use std::path::PathBuf;

use collections::VecDeque;
use serde::{Deserialize, Serialize};

use crate::state::Mode;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum NeovimData {
    Put { state: String },
    Key(String),
    Get { state: String, mode: Mode },
    ReadRegister { name: char, value: String },
    Exec { command: String },
    SetOption { value: String },
}

fn test_data_path(test_case_id: &str) -> PathBuf {
    let mut data_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    data_path.push("test_data");
    data_path.push(format!("{}.json", test_case_id));
    data_path
}

#[cfg(not(feature = "neovim"))]
pub fn read_test_data(test_case_id: &str) -> VecDeque<NeovimData> {
    let path = test_data_path(test_case_id);
    let json = std::fs::read_to_string(path).expect(
        "Could not read test data. Is it generated? Try running test with '--features neovim'",
    );

    let mut result = VecDeque::new();
    for line in json.lines() {
        result.push_back(
            serde_json::from_str(line)
                .expect("invalid test data. regenerate it with '--features neovim'"),
        );
    }
    result
}

#[cfg(feature = "neovim")]
pub fn write_test_data(test_case_id: &str, data: &VecDeque<NeovimData>) {
    let path = test_data_path(test_case_id);
    let mut json = Vec::new();
    for entry in data {
        serde_json::to_writer(&mut json, entry).unwrap();
        json.push(b'\n');
    }
    std::fs::create_dir_all(path.parent().unwrap()).expect("could not create test data directory");
    std::fs::write(path, json).expect("could not write out test data");
}
