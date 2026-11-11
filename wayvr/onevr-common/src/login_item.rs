//! Starting oneVR at login: a desktop entry in `$XDG_CONFIG_HOME/autostart` that runs
//! `onevr --tray`, the server with oneVR in the tray and no window.

use std::path::{Path, PathBuf};

const FILE: &str = "io.github.onevr.desktop";
/// oneVR's icon, under the install prefix, as the app menu's entry names it
const ICON: &str = "share/icons/hicolor/scalable/apps/io.github.onevr.svg";

/// Where the entry goes, None without HOME
pub fn path() -> Option<PathBuf> {
	Some(
		crate::xdg_dir("XDG_CONFIG_HOME", ".config")?
			.join("autostart")
			.join(FILE),
	)
}

/// Whether oneVR starts at login: the entry is there and not hidden, as a desktop's own
/// startup settings may hide it instead of removing it
pub fn enabled() -> bool {
	path().is_some_and(|path| enabled_at(&path))
}

fn enabled_at(path: &Path) -> bool {
	std::fs::read_to_string(path).is_ok_and(|entry| {
		!entry.lines().any(|line| {
			let line = line.replace(' ', "");
			line.eq_ignore_ascii_case("Hidden=true") || line.eq_ignore_ascii_case("X-GNOME-Autostart-enabled=false")
		})
	})
}

/// Adds or removes the entry. It names this install's `onevr`, so it starts the oneVR that
/// turned it on.
pub fn set(enable: bool) -> std::io::Result<()> {
	let path = path().ok_or_else(|| std::io::Error::other("HOME isn't set"))?;
	let icon = crate::install_prefix()
		.map(|prefix| prefix.join(ICON))
		.filter(|icon| icon.is_file());
	set_at(&path, enable, &crate::onevr_command(), icon.as_deref())
}

fn set_at(path: &Path, enable: bool, onevr: &Path, icon: Option<&Path>) -> std::io::Result<()> {
	if !enable {
		return match std::fs::remove_file(path) {
			Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
			result => result,
		};
	}
	if let Some(dir) = path.parent() {
		std::fs::create_dir_all(dir)?;
	}
	let icon = icon.map_or_else(|| "io.github.onevr".to_string(), |icon| icon.display().to_string());
	let entry = format!(
		"[Desktop Entry]\nType=Application\nName=oneVR\nComment=Starts oneVR in the tray\nExec={} --tray\nIcon={icon}\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
		exec_arg(&onevr.display().to_string())
	);
	crate::write_file(path, entry)
}

/// A program path for Exec: quoted when it has a character the desktop entry spec reserves
fn exec_arg(arg: &str) -> String {
	const RESERVED: &[char] = &[
		' ', '\t', '\n', '"', '\'', '\\', '>', '<', '~', '|', '&', ';', '$', '*', '?', '#', '(', ')', '`',
	];
	if !arg.contains(RESERVED) {
		return arg.to_string();
	}
	let mut quoted = String::from("\"");
	for c in arg.chars() {
		if matches!(c, '"' | '`' | '$' | '\\') {
			// escaped for Exec, then the backslash again for the string's own escapes
			quoted.push_str("\\\\");
		}
		quoted.push(c);
	}
	quoted.push('"');
	// a literal % is %% in Exec
	quoted.replace('%', "%%")
}

#[cfg(test)]
mod tests {
	use super::*;

	fn temp_dir(name: &str) -> PathBuf {
		let dir = std::env::temp_dir().join(format!("onevr-login-item-{name}-{}", std::process::id()));
		let _ = std::fs::remove_dir_all(&dir);
		dir
	}

	#[test]
	fn adds_and_removes_the_entry() {
		let dir = temp_dir("toggle");
		let path = dir.join("autostart").join(FILE);
		assert!(!enabled_at(&path));

		set_at(&path, true, Path::new("/opt/onevr/bin/onevr"), None).unwrap();
		assert!(enabled_at(&path));
		let entry = std::fs::read_to_string(&path).unwrap();
		assert!(entry.contains("\nExec=/opt/onevr/bin/onevr --tray\n"), "{entry}");
		assert!(entry.contains("\nIcon=io.github.onevr\n"), "{entry}");

		set_at(&path, false, Path::new("/opt/onevr/bin/onevr"), None).unwrap();
		assert!(!path.exists());
		// removing it again is fine
		set_at(&path, false, Path::new("/opt/onevr/bin/onevr"), None).unwrap();
		std::fs::remove_dir_all(dir).unwrap();
	}

	#[test]
	fn a_hidden_entry_is_off() {
		let dir = temp_dir("hidden");
		std::fs::create_dir_all(&dir).unwrap();
		let path = dir.join(FILE);
		std::fs::write(&path, "[Desktop Entry]\nExec=onevr --tray\nHidden=true\n").unwrap();
		assert!(!enabled_at(&path));
		std::fs::write(
			&path,
			"[Desktop Entry]\nExec=onevr --tray\nX-GNOME-Autostart-enabled=false\n",
		)
		.unwrap();
		assert!(!enabled_at(&path));
		std::fs::remove_dir_all(dir).unwrap();
	}

	#[test]
	fn quotes_a_path_with_spaces() {
		assert_eq!(exec_arg("/opt/onevr/bin/onevr"), "/opt/onevr/bin/onevr");
		assert_eq!(exec_arg("/home/a b/onevr"), "\"/home/a b/onevr\"");
		assert_eq!(exec_arg("/x$y z/onevr"), "\"/x\\\\$y z/onevr\"");
	}
}
