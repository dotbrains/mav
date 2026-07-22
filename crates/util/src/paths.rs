use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use itertools::Itertools;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::error::Error;
use std::fmt::Formatter;
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    sync::LazyLock,
};

use crate::rel_path::RelPath;
use crate::rel_path::RelPathBuf;

mod path_style;
mod sanitized_path;

pub use path_style::{PathStyle, RemotePathBuf, is_absolute};
pub use sanitized_path::SanitizedPath;

mod lexical;
mod matcher;
mod path_ext;
mod position;
mod sorting;
mod url_ext;
mod wsl;

pub use lexical::*;
pub use matcher::*;
pub use path_ext::*;
pub use position::*;
pub use sorting::*;
pub use url_ext::*;
pub use wsl::*;

#[cfg(test)]
mod tests;
