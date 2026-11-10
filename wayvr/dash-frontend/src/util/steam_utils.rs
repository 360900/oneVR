use keyvalues_parser::{Obj, Vdf};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, path::PathBuf};

#[derive(Clone)]
pub struct SteamUtils {
	steam_root: PathBuf,
}

fn get_steam_root() -> anyhow::Result<PathBuf> {
	let home = PathBuf::from(std::env::var("HOME")?);

	let steam_paths: [&str; 3] = [
		".steam/steam",
		".steam/debian-installation",
		".var/app/com.valvesoftware.Steam/data/Steam",
	];
	let Some(steam_path) = steam_paths.iter().map(|path| home.join(path)).find(|p| p.exists()) else {
		anyhow::bail!("Couldn't find Steam installation in search paths");
	};

	Ok(steam_path)
}

pub type AppID = String;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppManifest {
	pub app_id: AppID,
	pub run_game_id: AppID,
	pub name: String,
	pub raw_state_flags: u64, // documentation: https://github.com/lutris/lutris/blob/master/docs/steam.rst
	pub last_played: Option<u64>, // unix timestamp
	/// oneVR: a Steam tool such as Proton or a Steam Linux Runtime
	#[serde(default)]
	pub tool: bool,
}

// TODO @oo8dev: game sort methods
#[allow(dead_code)]
pub enum GameSortMethod {
	None,
	NameAsc,
	NameDesc,
	PlayDateDesc,
}

fn get_obj_first<'a>(obj: &'a Obj<'_>, key: &str) -> Option<&'a Obj<'a>> {
	obj.get(key)?.first()?.get_obj()
}

fn get_str_first<'a>(obj: &'a Obj<'_>, key: &str) -> Option<&'a str> {
	obj.get(key)?.first()?.get_str()
}

fn vdf_parse_libraryfolders<'a>(vdf_root: &'a Vdf<'a>) -> Option<Vec<AppEntry>> {
	let obj_libraryfolders = vdf_root.value.get_obj()?;

	let mut res = Vec::<AppEntry>::new();

	let mut num = 0;

	#[allow(clippy::while_let_loop)]
	loop {
		let Some(library_folder) = get_obj_first(obj_libraryfolders, format!("{}", num).as_str()) else {
			// no more libraries to find
			break;
		};

		let Some(apps) = get_obj_first(library_folder, "apps") else {
			// no apps?
			num += 1;
			continue;
		};

		let Some(path) = get_str_first(library_folder, "path") else {
			// no path?
			num += 1;
			continue;
		};

		//log::trace!("path: {}", path);

		res.extend(
			apps
				.iter()
				.filter_map(|item| item.0.parse::<u64>().ok())
				.map(|app_id| AppEntry {
					app_id: app_id.to_string(),
					root_path: String::from(path),
				}),
		);

		num += 1;
	}

	Some(res)
}

fn vdf_parse_appstate<'a>(app_id: AppID, vdf_root: &'a Vdf<'a>) -> Option<AppManifest> {
	let app_state_obj = vdf_root.value.get_obj()?;

	let name = app_state_obj.get("name")?.first()?.get_str()?;

	let raw_state_flags = app_state_obj
		.get("StateFlags")?
		.first()?
		.get_str()?
		.parse::<u64>()
		.ok()?;

	let last_played = match app_state_obj.get("LastPlayed") {
		Some(s) => Some(s.first()?.get_str()?.parse::<u64>().ok()?),
		None => None,
	};

	Some(AppManifest {
		app_id: app_id.clone(),
		run_game_id: app_id,
		name: String::from(name),
		raw_state_flags,
		last_played,
		// oneVR: set by get_app_manifest, which knows the library folder
		tool: false,
	})
}

struct AppEntry {
	pub root_path: String,
	pub app_id: AppID,
}

/// oneVR: a library folder in libraryfolders.vdf and the apps it lists
struct LibraryFolder {
	path: PathBuf,
	apps: HashSet<AppID>,
}

/// oneVR: every library folder with a path, also one that lists no apps
fn vdf_library_folders(vdf_root: &Vdf<'_>) -> Option<Vec<LibraryFolder>> {
	let folders = vdf_root.value.get_obj()?;
	let folders = folders
		.iter()
		.filter(|(key, _)| key.parse::<u64>().is_ok())
		.filter_map(|(_, values)| {
			let folder = values.first()?.get_obj()?;
			let apps = get_obj_first(folder, "apps")
				.map(|apps| {
					apps
						.keys()
						.filter_map(|id| Some(id.parse::<u64>().ok()?.to_string()))
						.collect()
				})
				.unwrap_or_default();
			Some(LibraryFolder {
				path: PathBuf::from(get_str_first(folder, "path")?),
				apps,
			})
		})
		.collect();
	Some(folders)
}

/// oneVR: of `app_ids`, the ones no library folder holds a manifest for. Steam removes the manifest
/// at once on an uninstall but rewrites libraryfolders.vdf much later. An app a folder out of reach
/// lists, as on a drive that isn't mounted, may still be installed.
fn uninstalled_in<'a>(folders: &[LibraryFolder], app_ids: impl IntoIterator<Item = &'a str>) -> Vec<AppID> {
	app_ids
		.into_iter()
		.filter(|id| {
			folders.iter().all(|folder| {
				let steamapps = folder.path.join("steamapps");
				if !steamapps.is_dir() {
					return !folder.apps.contains(*id);
				}
				// an error reading it counts as installed
				steamapps
					.join(format!("appmanifest_{id}.acf"))
					.try_exists()
					.is_ok_and(|exists| !exists)
			})
		})
		.map(str::to_owned)
		.collect()
}

pub fn launch(app_id: &AppID) -> anyhow::Result<()> {
	log::info!("Launching Steam game with AppID {}", app_id);
	launch_with_arguments(app_id, "")
}

/// oneVR: `-applaunch`, as `steam://rungameid` opens Steam's launch option dialog on the desktop,
/// out of reach in the headset. See [`super::steam_launch`].
pub fn launch_with_arguments(app_id: &AppID, arguments: &str) -> anyhow::Result<()> {
	let flatpak = get_steam_root().is_ok_and(|root| root.starts_with(flatpak_root()));
	let mut command = if flatpak {
		let mut command = std::process::Command::new("flatpak");
		command.args(["run", "com.valvesoftware.Steam"]);
		command
	} else {
		std::process::Command::new("steam")
	};
	command
		.args(["-applaunch", app_id])
		.args(split_arguments(arguments))
		.spawn()?;
	Ok(())
}

/// oneVR: a launch entry's arguments are a command line, where quotes keep an argument with
/// spaces together; one with unbalanced quotes is split at its spaces
fn split_arguments(arguments: &str) -> Vec<String> {
	shell_words::split(arguments).unwrap_or_else(|_| arguments.split_whitespace().map(str::to_string).collect())
}

fn flatpak_root() -> PathBuf {
	PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".var/app/com.valvesoftware.Steam")
}

pub fn stop(app_id: AppID, force_kill: bool) -> anyhow::Result<()> {
	log::info!("Stopping Steam game with AppID {}", app_id);

	for game in list_running_games()? {
		if game.app_id != app_id {
			continue;
		}

		log::info!("Killing process with PID {} and its children", game.pid);
		let _ = std::process::Command::new("pkill")
			.arg(if force_kill { "-9" } else { "-15" })
			.arg("-P")
			.arg(format!("{}", game.pid))
			.spawn()?;
	}
	Ok(())
}

#[derive(Serialize)]
pub struct RunningGame {
	pub app_id: AppID,
	pub pid: i32,
}

pub fn list_running_games() -> anyhow::Result<Vec<RunningGame>> {
	let mut res = Vec::<RunningGame>::new();

	let entries = std::fs::read_dir("/proc")?;
	for entry in entries.into_iter().flatten() {
		let path_cmdline = entry.path().join("cmdline");
		let Ok(cmdline) = std::fs::read(path_cmdline) else {
			continue;
		};

		let proc_file_name = entry.file_name();
		let Some(pid) = proc_file_name.to_str() else {
			continue;
		};

		let Ok(pid) = pid.parse::<i32>() else {
			continue;
		};

		let args: Vec<&str> = cmdline
			.split(|byte| *byte == 0x00)
			.filter_map(|arg| std::str::from_utf8(arg).ok())
			.collect();

		let mut has_steam_launch = false;
		for arg in &args {
			if *arg == "SteamLaunch" {
				has_steam_launch = true;
				break;
			}
		}

		if !has_steam_launch {
			continue;
		}

		// Running game process found. Parse AppID
		for arg in &args {
			let pat = "AppId=";
			let Some(pos) = arg.find(pat) else {
				continue;
			};

			if pos != 0 {
				continue;
			}

			let Some((_, second)) = arg.split_at_checked(pat.len()) else {
				continue;
			};

			let Ok(app_id_num) = second.parse::<u64>() else {
				continue;
			};

			// AppID found. Add it to the list
			res.push(RunningGame {
				app_id: app_id_num.to_string(),
				pid,
			});

			break;
		}
	}

	Ok(res)
}

impl SteamUtils {
	/// oneVR
	pub fn root(&self) -> &std::path::Path {
		&self.steam_root
	}

	fn get_dir_steamapps(&self) -> PathBuf {
		self.steam_root.join("steamapps")
	}

	pub fn new() -> anyhow::Result<Self> {
		let steam_root = get_steam_root()?;

		Ok(Self { steam_root })
	}

	fn get_app_manifest(&self, app_entry: &AppEntry) -> anyhow::Result<AppManifest> {
		let manifest_path =
			PathBuf::from(&app_entry.root_path).join(format!("steamapps/appmanifest_{}.acf", app_entry.app_id));

		let vdf_data = std::fs::read_to_string(manifest_path)?;
		let vdf_root = keyvalues_parser::parse(&vdf_data)?.into_vdf();

		let Some(mut manifest) = vdf_parse_appstate(app_entry.app_id.clone(), &vdf_root) else {
			anyhow::bail!("Failed to parse AppState");
		};

		// oneVR: every Proton build and Steam Linux Runtime ships a toolmanifest.vdf
		if let Some(app_state) = vdf_root.value.get_obj()
			&& let Some(install_dir) = get_str_first(app_state, "installdir")
		{
			manifest.tool = PathBuf::from(&app_entry.root_path)
				.join("steamapps/common")
				.join(install_dir)
				.join("toolmanifest.vdf")
				.is_file();
		}

		Ok(manifest)
	}

	/// oneVR: of `app_ids`, the Steam games that were uninstalled; an error when Steam's library
	/// folders can't be read, so nothing counts as uninstalled then
	pub fn uninstalled<'a>(&self, app_ids: impl IntoIterator<Item = &'a str>) -> anyhow::Result<Vec<AppID>> {
		let path = self.get_dir_steamapps().join("libraryfolders.vdf");
		let vdf_data = std::fs::read_to_string(path)?;
		let vdf_root = keyvalues_parser::parse(&vdf_data)?.into_vdf();
		let folders = vdf_library_folders(&vdf_root).unwrap_or_default();
		anyhow::ensure!(!folders.is_empty(), "libraryfolders.vdf lists no library folder");
		Ok(uninstalled_in(&folders, app_ids))
	}

	pub fn list_installed_games(&self, sort_method: GameSortMethod) -> anyhow::Result<Vec<AppManifest>> {
		let path = self.get_dir_steamapps().join("libraryfolders.vdf");
		let vdf_data = std::fs::read_to_string(path)?;

		let vdf_root = keyvalues_parser::parse(&vdf_data)?.into_vdf();

		let Some(apps) = vdf_parse_libraryfolders(&vdf_root) else {
			anyhow::bail!("Failed to fetch installed Steam apps");
		};

		let mut games: Vec<AppManifest> = apps
			.iter()
			.filter_map(|app_entry| {
				let manifest = match self.get_app_manifest(app_entry) {
					Ok(manifest) => manifest,
					Err(e) => {
						log::warn!(
							"Failed to get app manifest for AppID {}: {}. This entry won't show.",
							app_entry.app_id,
							e
						);
						return None;
					}
				};
				Some(manifest)
			})
			.collect();

		match sort_method {
			GameSortMethod::None => {}
			GameSortMethod::NameAsc => {
				games.sort_by(|a, b| a.name.cmp(&b.name));
			}
			GameSortMethod::NameDesc => {
				games.sort_by(|a, b| b.name.cmp(&a.name));
			}
			GameSortMethod::PlayDateDesc => {
				games.sort_by_key(|b| std::cmp::Reverse(b.last_played));
			}
		}

		Ok(games)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn launch_arguments_keep_quoted_spaces() {
		assert_eq!(split_arguments(""), Vec::<String>::new());
		assert_eq!(split_arguments("-vr  -novid"), ["-vr", "-novid"]);
		assert_eq!(
			split_arguments(r#"-config "My Settings.ini" -vr"#),
			["-config", "My Settings.ini", "-vr"]
		);
		// unbalanced quotes
		assert_eq!(split_arguments(r#"-name "Jane -vr"#), ["-name", "\"Jane", "-vr"]);
	}

	#[test]
	fn uninstalled_games_have_no_manifest() {
		let dir = std::env::temp_dir().join(format!("onevr-steam-{}", std::process::id()));
		let (mounted, unmounted) = (dir.join("mounted"), dir.join("unmounted"));
		std::fs::create_dir_all(mounted.join("steamapps")).unwrap();
		std::fs::write(mounted.join("steamapps/appmanifest_620.acf"), "").unwrap();
		let vdf_data = format!(
			"\"libraryfolders\"\n{{\n\t\"0\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t\t\"apps\"\n\t\t{{\n\t\t\t\"620\"\t\t\"1\"\n\t\t\t\"570\"\t\t\"1\"\n\t\t}}\n\t}}\n\t\"1\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t\t\"apps\"\n\t\t{{\n\t\t\t\"440\"\t\t\"1\"\n\t\t}}\n\t}}\n}}\n",
			mounted.display(),
			unmounted.display()
		);
		let vdf_root = keyvalues_parser::parse(&vdf_data).unwrap().into_vdf();
		let folders = vdf_library_folders(&vdf_root).unwrap();
		assert_eq!(folders.len(), 2);

		// 620 is installed, 570 is listed without a manifest, 440 may be on the unmounted drive and
		// 730 is listed nowhere
		let uninstalled = uninstalled_in(&folders, ["620", "570", "440", "730"]);
		assert_eq!(uninstalled, ["570", "730"]);

		std::fs::remove_dir_all(&dir).unwrap();
	}
}
