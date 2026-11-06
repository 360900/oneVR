//! oneVR: a Steam game's launch entries, for picking one without Steam's dialog. A game can list
//! several for the same launch type, and Steam then opens a dialog on the desktop, out of reach
//! in the headset. `steam -applaunch <id> <arguments>` never opens it: it runs the first entry
//! with those arguments, which also makes it the way to pick another entry.

use std::{
	collections::HashMap,
	path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchOption {
	/// The entry's key in `appinfo.vdf`, which is also the number Steam saves as its default
	pub key: String,
	/// What it is called, None if Steam has no name for it
	pub description: Option<String>,
	/// `vr`, `openxr`, or a non-VR type
	pub kind: String,
	pub arguments: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SteamLaunch {
	/// Talks to the OpenXR runtime directly, so xrizer's settings don't reach it
	pub openxr_only: bool,
	/// What can be picked: entries `-applaunch` can reach, in Steam's order
	pub options: Vec<LaunchOption>,
}

#[derive(Debug, PartialEq, Eq)]
enum Value {
	Dict(HashMap<String, Value>),
	Str(String),
	Int(u32),
}

impl Value {
	fn get(&self, key: &str) -> Option<&Value> {
		match self {
			Self::Dict(dict) => dict.get(key),
			_ => None,
		}
	}

	fn text(&self, key: &str) -> Option<&str> {
		match self.get(key)? {
			Self::Str(text) => Some(text),
			_ => None,
		}
	}

	/// Steam stores these as a number for some games and as text for others
	fn flag(&self, key: &str) -> bool {
		match self.get(key) {
			Some(Self::Int(value)) => *value != 0,
			Some(Self::Str(text)) => !text.is_empty() && text != "0",
			_ => false,
		}
	}

	fn int_or_text(&self, key: &str) -> Option<String> {
		match self.get(key)? {
			Self::Int(value) => Some(value.to_string()),
			Self::Str(text) => Some(text.clone()),
			Self::Dict(_) => None,
		}
	}
}

struct Reader<'a> {
	data: &'a [u8],
	pos: usize,
	strings: Option<Vec<String>>,
}

impl Reader<'_> {
	fn bytes(&mut self, count: usize) -> Option<&[u8]> {
		let bytes = self.data.get(self.pos..self.pos.checked_add(count)?)?;
		self.pos += count;
		Some(bytes)
	}

	fn u32(&mut self) -> Option<u32> {
		Some(u32::from_le_bytes(self.bytes(4)?.try_into().ok()?))
	}

	fn text(&mut self) -> Option<String> {
		let rest = self.data.get(self.pos..)?;
		let end = rest.iter().position(|b| *b == 0)?;
		let text = String::from_utf8_lossy(&rest[..end]).into_owned();
		self.pos += end + 1;
		Some(text)
	}

	fn key(&mut self) -> Option<String> {
		if self.strings.is_some() {
			let index = self.u32()? as usize;
			self.strings.as_ref()?.get(index).cloned()
		} else {
			self.text()
		}
	}

	fn dict(&mut self, depth: u32) -> Option<Value> {
		if depth > 32 {
			return None;
		}
		let mut dict = HashMap::new();
		loop {
			let kind = *self.bytes(1)?.first()?;
			if kind == 8 {
				return Some(Value::Dict(dict));
			}
			let key = self.key()?;
			let value = match kind {
				0 => self.dict(depth + 1)?,
				1 => Value::Str(self.text()?),
				2 => Value::Int(self.u32()?),
				_ => return None,
			};
			dict.insert(key, value);
		}
	}
}

/// Header of an app, after its id and size: state, last update, token, two hashes, change number,
/// checksum and type. See <https://github.com/SteamDatabase/SteamAppInfo>
const APP_HEADER_SIZE: usize = 4 + 4 + 8 + 20 + 4 + 20 + 4 + 1;
const MAGIC_BASE: u32 = 0x0756_4400;
const FIRST_VERSION_WITH_STRING_TABLE: u32 = 41;

/// What `appinfo.vdf` says about the apps in `ids`; an app it doesn't know is missing
fn parse_appinfo(data: &[u8], ids: &[u64]) -> Option<HashMap<u64, SteamLaunch>> {
	let mut reader = Reader {
		data,
		pos: 0,
		strings: None,
	};
	let magic = reader.u32()?;
	if magic & 0xffff_ff00 != MAGIC_BASE {
		return None;
	}
	reader.u32()?; // universe
	if magic & 0xff >= FIRST_VERSION_WITH_STRING_TABLE {
		let offset = u64::from_le_bytes(reader.bytes(8)?.try_into().ok()?);
		let start = reader.pos;
		reader.pos = usize::try_from(offset).ok()?;
		let count = reader.u32()? as usize;
		let mut strings = Vec::with_capacity(count.min(1 << 20));
		for _ in 0..count {
			strings.push(reader.text()?);
		}
		reader.strings = Some(strings);
		reader.pos = start;
	}

	let mut apps = HashMap::new();
	loop {
		let id = reader.u32()?;
		if id == 0 {
			return Some(apps);
		}
		let size = reader.u32()? as usize;
		let next = reader.pos.checked_add(size)?;
		if ids.contains(&u64::from(id)) {
			reader.pos += APP_HEADER_SIZE;
			// one app that doesn't parse leaves the others
			if let Some(info) = reader.dict(0) {
				let info = info.get("appinfo").unwrap_or(&info);
				apps.insert(u64::from(id), steam_launch(info));
			}
		}
		reader.pos = next;
	}
}

fn steam_launch(info: &Value) -> SteamLaunch {
	let common = info.get("common");
	let flag = |key| common.is_some_and(|c| c.flag(key));
	let (openvr, openxr) = (flag("openvrsupport"), flag("openxrsupport"));
	SteamLaunch {
		openxr_only: openxr && !openvr,
		options: info
			.get("config")
			.and_then(|c| c.get("launch"))
			.map_or_else(Vec::new, |launch| options(launch, openvr || openxr)),
	}
}

/// The entries that can start here and that `-applaunch` can tell apart
fn options(launch: &Value, vr: bool) -> Vec<LaunchOption> {
	let Value::Dict(entries) = launch else {
		return Vec::new();
	};
	let mut sorted: Vec<_> = entries.iter().collect();
	sorted.sort_by_key(|(key, _)| key.parse::<u32>().unwrap_or(u32::MAX));

	// `-applaunch` runs the first entry that runs here, and only its arguments can change
	let first_key = sorted.iter().find(|(_, entry)| runs_here(entry)).map(|(key, _)| *key);
	let usable: Vec<_> = sorted
		.into_iter()
		.filter(|(_, entry)| runs_here(entry) && is_kind_offered(entry, vr))
		.collect();
	let Some((usable_key, first)) = usable.first() else {
		return Vec::new();
	};
	// it would start this other entry, such as the flat mode of a VR game: nothing can be picked
	let first_is_runnable = first_key == Some(*usable_key);
	let executable = first.text("executable").unwrap_or_default().to_lowercase();
	let oslist = first.get("config").and_then(|c| c.text("oslist"));
	let first_has_arguments = first.text("arguments").is_some_and(|a| !a.trim().is_empty());

	usable
		.iter()
		.enumerate()
		.filter(|(index, (_, entry))| {
			*index == 0
				|| (first_is_runnable
					&& !first_has_arguments
					&& entry.text("executable").unwrap_or_default().to_lowercase() == executable
					&& entry.get("config").and_then(|c| c.text("oslist")) == oslist)
		})
		.map(|(_, (key, entry))| LaunchOption {
			key: (*key).clone(),
			description: entry
				.text("description")
				.filter(|d| !d.is_empty() && !d.starts_with('#'))
				.map(str::to_owned),
			kind: entry.text("type").unwrap_or("default").to_lowercase(),
			arguments: entry.text("arguments").unwrap_or_default().to_owned(),
		})
		.collect()
}

fn runs_here(entry: &Value) -> bool {
	let config = entry.get("config");
	let text = |key| config.and_then(|c: &Value| c.text(key));
	// another branch's entry doesn't run on the installed one
	if config.is_some_and(|c| c.get("BetaKey").is_some()) {
		return false;
	}
	if text("oslist").is_some_and(|os| !os.contains("windows") && !os.contains("linux")) {
		return false;
	}
	config.and_then(|c| c.int_or_text("osarch")).as_deref() != Some("32")
}

fn is_kind_offered(entry: &Value, vr: bool) -> bool {
	let kind = entry.text("type").unwrap_or("default").to_lowercase();
	if vr {
		matches!(kind.as_str(), "vr" | "openxr")
	} else {
		!matches!(
			kind.as_str(),
			"server" | "editor" | "config" | "manual" | "benchmark" | "othervr"
		)
	}
}

/// What Steam saved as a game's default in `localconfig.vdf`, under `DefaultLaunchOption`. It is
/// kept per list of options: with more than one the current one is unknown.
fn parse_defaults(localconfig: &str) -> HashMap<u64, String> {
	let mut defaults = HashMap::new();
	let mut lines = localconfig.lines().map(str::trim);
	let mut app = None;
	while let Some(line) = lines.next() {
		if line == "\"DefaultLaunchOption\"" {
			let mut values = Vec::new();
			for inner in lines.by_ref() {
				if inner == "}" {
					break;
				}
				let parts: Vec<_> = inner.split('"').collect();
				if let [_, _, _, value, _] = parts[..] {
					values.push(value.to_owned());
				}
			}
			// -1 is "Ask when starting game"
			if let (Some(id), [value]) = (app, &values[..])
				&& value.parse::<u32>().is_ok()
			{
				defaults.insert(id, value.clone());
			}
		} else if let Some(id) = line
			.strip_prefix('"')
			.and_then(|l| l.strip_suffix('"'))
			.and_then(|l| l.parse::<u64>().ok())
		{
			app = Some(id);
		}
	}
	defaults
}

/// What is known about Steam's launch entries
#[derive(Default)]
pub struct SteamLaunchInfo {
	pub apps: HashMap<u64, SteamLaunch>,
	/// What the user made the default in Steam
	pub defaults: HashMap<u64, String>,
}

/// Empty if Steam's files can't be read, and then everything starts as without them
pub fn read(root: &Path, ids: &[u64]) -> SteamLaunchInfo {
	let apps = std::fs::read(root.join("appcache/appinfo.vdf"))
		.ok()
		.and_then(|data| parse_appinfo(&data, ids))
		.unwrap_or_default();
	let defaults = newest_localconfig(root)
		.and_then(|path| std::fs::read(path).ok())
		.map(|data| parse_defaults(&String::from_utf8_lossy(&data)))
		.unwrap_or_default();
	SteamLaunchInfo { apps, defaults }
}

/// The last user's, who is the one logged in
fn newest_localconfig(root: &Path) -> Option<PathBuf> {
	std::fs::read_dir(root.join("userdata"))
		.ok()?
		.flatten()
		.map(|user| user.path().join("config/localconfig.vdf"))
		.filter_map(|path| Some((path.metadata().ok()?.modified().ok()?, path)))
		.max_by_key(|(modified, _)| *modified)
		.map(|(_, path)| path)
}

#[cfg(test)]
mod tests {
	use super::*;

	fn entry(kind: Option<&str>, exe: &str, args: Option<&str>, os: Option<&str>, beta: bool) -> Value {
		let mut config = HashMap::new();
		if let Some(os) = os {
			config.insert("oslist".to_owned(), Value::Str(os.to_owned()));
		}
		if beta {
			config.insert("BetaKey".to_owned(), Value::Str("beta".to_owned()));
		}
		let mut dict = HashMap::from([
			("executable".to_owned(), Value::Str(exe.to_owned())),
			("config".to_owned(), Value::Dict(config)),
		]);
		if let Some(kind) = kind {
			dict.insert("type".to_owned(), Value::Str(kind.to_owned()));
		}
		if let Some(args) = args {
			dict.insert("arguments".to_owned(), Value::Str(args.to_owned()));
		}
		Value::Dict(dict)
	}

	fn launch(entries: Vec<(&str, Value)>) -> Value {
		Value::Dict(entries.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
	}

	fn keys(options: &[LaunchOption]) -> Vec<&str> {
		options.iter().map(|o| o.key.as_str()).collect()
	}

	#[test]
	fn pavlov_has_two() {
		let launch = launch(vec![
			("0", entry(Some("openxr"), "Pavlov.exe", None, Some("windows"), false)),
			(
				"1",
				entry(Some("openxr"), "Pavlov.exe", Some("-dx11"), Some("windows"), false),
			),
		]);
		let options = options(&launch, true);
		assert_eq!(keys(&options), ["0", "1"]);
		assert_eq!(options[1].arguments, "-dx11");
	}

	#[test]
	fn other_systems_branches_and_non_vr_entries_are_left_out() {
		let launch = launch(vec![
			("0", entry(Some("vr"), "launch.exe", None, Some("windows"), false)),
			(
				"1",
				entry(Some("option1"), "launch.exe", Some("--no-vr"), Some("windows"), false),
			),
			("2", entry(Some("openxr"), "game.apk", None, Some("android"), false)),
			("3", entry(Some("openxr"), "launch.exe", None, Some("windows"), true)),
			("4", entry(Some("othervr"), "launch.exe", None, Some("windows"), false)),
		]);
		assert_eq!(keys(&options(&launch, true)), ["0"]);
	}

	#[test]
	fn a_non_vr_game_lists_its_normal_entries() {
		let launch = launch(vec![
			("0", entry(None, "game.exe", None, Some("windows"), false)),
			(
				"1",
				entry(Some("option1"), "game.exe", Some("-safe"), Some("windows"), false),
			),
			(
				"2",
				entry(Some("editor"), "game.exe", Some("-edit"), Some("windows"), false),
			),
		]);
		assert_eq!(keys(&options(&launch, false)), ["0", "1"]);
	}

	#[test]
	fn an_entry_applaunch_cant_reach_is_left_out() {
		// another executable
		let other_exe = launch(vec![
			("0", entry(Some("vr"), "a.exe", None, None, false)),
			("1", entry(Some("openxr"), "b.exe", None, None, false)),
		]);
		assert_eq!(keys(&options(&other_exe, true)), ["0"]);
		// the first entry's arguments would come along
		let first_has_arguments = launch(vec![
			("0", entry(Some("vr"), "a.exe", Some("-vrmode openvr"), None, false)),
			("1", entry(Some("openxr"), "a.exe", Some("-vrmode openxr"), None, false)),
		]);
		assert_eq!(keys(&options(&first_has_arguments, true)), ["0"]);
	}

	#[test]
	fn nothing_is_offered_when_applaunch_would_run_a_flat_entry_first() {
		let launch = launch(vec![
			("0", entry(None, "a.exe", None, Some("windows"), false)),
			("1", entry(Some("vr"), "a.exe", Some("-vr"), Some("windows"), false)),
			("2", entry(Some("openxr"), "a.exe", Some("-xr"), Some("windows"), false)),
		]);
		assert_eq!(keys(&options(&launch, true)), ["1"]);
	}

	#[test]
	fn entries_sort_as_numbers() {
		let launch = launch(vec![
			("10", entry(Some("vr"), "a.exe", Some("-b"), None, false)),
			("2", entry(Some("vr"), "a.exe", None, None, false)),
		]);
		assert_eq!(keys(&options(&launch, true)), ["2", "10"]);
	}

	#[test]
	fn support_reads_as_number_and_text() {
		let info = |key: &str, value| {
			Value::Dict(HashMap::from([(
				"common".to_owned(),
				Value::Dict(HashMap::from([(key.to_owned(), value)])),
			)]))
		};
		assert!(steam_launch(&info("openxrsupport", Value::Str("1".to_owned()))).openxr_only);
		assert!(steam_launch(&info("openxrsupport", Value::Int(1))).openxr_only);
		assert!(!steam_launch(&info("openxrsupport", Value::Str("0".to_owned()))).openxr_only);
		assert!(!steam_launch(&info("openvrsupport", Value::Int(1))).openxr_only);
	}

	/// An `appinfo.vdf` as version 41 writes it
	#[test]
	fn reads_the_binary_format() {
		let strings = [
			"appinfo",
			"common",
			"openxrsupport",
			"config",
			"launch",
			"0",
			"executable",
			"type",
		];
		let key = |name: &str| (strings.iter().position(|s| *s == name).unwrap() as u32).to_le_bytes();
		let mut body = vec![0];
		body.extend(key("appinfo"));
		body.push(0);
		body.extend(key("common"));
		body.push(1);
		body.extend(key("openxrsupport"));
		body.extend(b"1\0");
		body.push(8);
		body.push(0);
		body.extend(key("config"));
		body.push(0);
		body.extend(key("launch"));
		body.push(0);
		body.extend(key("0"));
		body.push(1);
		body.extend(key("executable"));
		body.extend(b"Game.exe\0");
		body.push(1);
		body.extend(key("type"));
		body.extend(b"openxr\0");
		body.extend([8, 8, 8, 8, 8]);

		let mut app = vec![0; APP_HEADER_SIZE];
		app.extend(body);
		let mut apps = Vec::new();
		for id in [7u32, 555_160] {
			apps.extend(id.to_le_bytes());
			apps.extend((app.len() as u32).to_le_bytes());
			apps.extend(&app);
		}
		apps.extend(0u32.to_le_bytes());

		let mut file = (MAGIC_BASE | 41).to_le_bytes().to_vec();
		file.extend(1u32.to_le_bytes());
		file.extend(((16 + apps.len()) as u64).to_le_bytes());
		file.extend(apps);
		file.extend((strings.len() as u32).to_le_bytes());
		for string in strings {
			file.extend(string.as_bytes());
			file.push(0);
		}

		let parsed = parse_appinfo(&file, &[555_160]).unwrap();
		assert_eq!(parsed.len(), 1);
		let game = &parsed[&555_160];
		assert!(game.openxr_only);
		assert_eq!(keys(&game.options), ["0"]);
		assert_eq!(game.options[0].kind, "openxr");
	}

	#[test]
	fn a_wrong_file_is_refused() {
		assert!(parse_appinfo(b"not steam", &[1]).is_none());
		assert!(parse_appinfo(&[], &[1]).is_none());
	}

	#[test]
	fn reads_steams_defaults() {
		let file = r#"
"apps"
{
	"555160"
	{
		"DefaultLaunchOption"
		{
			"f99c2c10"		"1"
		}
		"LastPlayed"		"1793986519"
	}
	"4000"
	{
		"DefaultLaunchOption"
		{
			"79ad9454"		"2"
			"3286387c"		"1"
		}
	}
	"629730"
	{
		"DefaultLaunchOption"
		{
			"aaaa"		"-1"
		}
	}
}"#;
		let defaults = parse_defaults(file);
		assert_eq!(defaults.get(&555_160).map(String::as_str), Some("1"));
		// which one applies is unknown
		assert!(!defaults.contains_key(&4000));
		// asks
		assert!(!defaults.contains_key(&629_730));
	}
}
