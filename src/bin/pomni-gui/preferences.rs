//! Preserve favorites/history when upgrading from the old application name.
use std::{
    fs,
    io::{self, Write},
    path::Path,
};

pub fn migrate(old: &Path, new: &Path) -> io::Result<()> {
    if new.exists() || !old.is_file() {
        return Ok(());
    }
    let contents = fs::read(old)?;
    if let Some(parent) = new.parent() {
        fs::create_dir_all(parent)?;
    }
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(new)
    {
        Ok(mut file) => file.write_all(&contents),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn imports_previous_storage_once_without_overwriting_either_copy() {
        let root = std::env::temp_dir().join(format!("pomni-migrate-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let old = root.join("old.ron");
        let new = root.join("pomni/app.ron");
        migrate(&old, &new).unwrap();
        assert!(!new.exists());
        fs::write(&old, b"previous favorites and history").unwrap();
        migrate(&old, &new).unwrap();
        assert_eq!(fs::read(&new).unwrap(), fs::read(&old).unwrap());
        fs::write(&new, b"new Pomni preferences").unwrap();
        migrate(&old, &new).unwrap();
        assert_eq!(fs::read(&new).unwrap(), b"new Pomni preferences");
        assert_eq!(fs::read(&old).unwrap(), b"previous favorites and history");
        fs::remove_dir_all(root).unwrap();
    }
}
