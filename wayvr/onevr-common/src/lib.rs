//! oneVR integration shared by the WayVR crates.

use std::path::{Path, PathBuf};

pub mod added_apps;
pub mod clipboard;
pub mod doctor;
pub mod emulator;
pub mod headset;
pub mod library;
#[cfg(feature = "dbus")]
pub mod link;
pub mod login_item;
pub mod rebrand;
pub mod server_config;
pub mod state;
pub mod steam_env;
pub mod xrizer_apps;

/// This install's prefix, two levels above the running `wayvr` or `onevr-ui`
pub fn install_prefix() -> Option<PathBuf> {
	// /proc/self/exe is resolved already. Once an upgrade replaced the running file it ends in
	// " (deleted)", which canonicalize() fails on and parent() drops with the file name.
	Some(std::env::current_exe().ok()?.parent()?.parent()?.to_path_buf())
}

/// This install's `onevr`, next to the running `wayvr` or `onevr-ui`, else the one on PATH
pub fn onevr_command() -> PathBuf {
	install_prefix()
		.map(|prefix| prefix.join("bin/onevr"))
		.filter(|path| path.is_file())
		.unwrap_or_else(|| "onevr".into())
}

/// An XDG base directory: `var` if it's absolute, else `fallback` under HOME, as onevr's `xdg_dir`
/// finds it. None without HOME.
pub(crate) fn xdg_dir(var: &str, fallback: &str) -> Option<PathBuf> {
	let non_empty = |name| std::env::var_os(name).filter(|v| !v.is_empty());
	non_empty(var)
		.map(PathBuf::from)
		.filter(|dir| dir.is_absolute())
		.or_else(|| Some(PathBuf::from(non_empty("HOME")?).join(fallback)))
}

/// Writes a file whole or not at all: to a temporary file next to it, renamed over it. A crash
/// or a full disk in the middle leaves the old contents instead of a cut-off file. A symlink
/// stays, its target is replaced.
pub fn write_file(path: &Path, contents: impl AsRef<[u8]>) -> std::io::Result<()> {
	let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
	let mut name = path.file_name().unwrap_or_default().to_os_string();
	name.push(format!(".tmp{}", std::process::id()));
	let tmp = path.with_file_name(name);
	let written = std::fs::write(&tmp, contents).and_then(|()| std::fs::rename(&tmp, &path));
	if written.is_err() {
		let _ = std::fs::remove_file(&tmp);
	}
	written
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn write_file_replaces_whole_files() {
		let dir = std::env::temp_dir().join(format!("onevr-write-file-{}", std::process::id()));
		std::fs::create_dir_all(&dir).unwrap();
		let (file, link) = (dir.join("file"), dir.join("link"));
		write_file(&file, "one").unwrap();
		write_file(&file, "two").unwrap();
		assert_eq!(std::fs::read_to_string(&file).unwrap(), "two");
		// through a symlink, which stays one
		std::os::unix::fs::symlink(&file, &link).unwrap();
		write_file(&link, "three").unwrap();
		assert!(link.is_symlink());
		assert_eq!(std::fs::read_to_string(&file).unwrap(), "three");
		// nothing is left behind, also when the write fails
		assert!(write_file(&dir.join("missing/file"), "four").is_err());
		assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);
		std::fs::remove_dir_all(&dir).unwrap();
	}
}
