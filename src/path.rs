
use std::path::{Path, PathBuf};

pub fn get_home_relative_path(path: impl AsRef<Path>) -> PathBuf {
    let home = std::env::var("HOME").unwrap();
    let home = Path::new(&home);
    home.join(path)
}

