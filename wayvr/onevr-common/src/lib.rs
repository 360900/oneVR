//! oneVR integration shared by the WayVR crates.

use std::path::PathBuf;

pub mod added_apps;
pub mod clipboard;
pub mod doctor;
pub mod emulator;
pub mod headset;
pub mod library;
#[cfg(feature = "dbus")]
pub mod link;
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
