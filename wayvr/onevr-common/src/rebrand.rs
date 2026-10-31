//! Shows oneVR instead of WayVR in user-visible strings without editing upstream files.
//!
//! Asset providers load oneVR's own assets (`onevr-assets/` in each crate) before the upstream
//! ones, and pass language files through [`lang_json`].

use std::borrow::Cow;

use serde_json::Value;

const UPSTREAM_NAME: &str = "WayVR";
const NAME: &str = "oneVR";

/// oneVR's own strings live under this key
const ONEVR_KEY: &str = "ONEVR";

/// Translation keys left as they are, because they credit the WayVR project.
const KEEP_PREFIXES: &[&str] = &["DONATE."];

/// Loads `path` from oneVR's assets if present, otherwise from upstream. Language files are
/// always built from upstream, rebranded, with oneVR's strings merged over them; oneVR's own
/// strings fall back to English until translated. Upstream keys keep their translations, so
/// oneVR's `en.json` only adds strings under `ONEVR`: rewording an upstream key there would
/// change only English.
pub fn load(
	path: &str,
	onevr: impl Fn(&str) -> Option<Cow<'static, [u8]>>,
	upstream: impl Fn(&str) -> Option<Cow<'static, [u8]>>,
) -> anyhow::Result<Vec<u8>> {
	if path.starts_with("lang/") && path.ends_with(".json") {
		let Some(base) = upstream(path) else {
			anyhow::bail!("embedded file {path} not found");
		};
		let mut overrides = Vec::new();
		if path != "lang/en.json"
			&& let Some(english) = onevr("lang/en.json")
		{
			let english: Value = serde_json::from_slice(&english)?;
			overrides.push(serde_json::json!({ ONEVR_KEY: english[ONEVR_KEY] }));
		}
		if let Some(own) = onevr(path) {
			overrides.push(serde_json::from_slice(&own)?);
		}
		return lang_json(&base, overrides);
	}
	match onevr(path).or_else(|| upstream(path)) {
		Some(data) => Ok(data.into_owned()),
		None => anyhow::bail!("embedded file {path} not found"),
	}
}

/// Rebrands an upstream language file and merges oneVR's strings (`overrides`, in order) over it.
pub fn lang_json(upstream: &[u8], overrides: Vec<Value>) -> anyhow::Result<Vec<u8>> {
	let mut root: Value = serde_json::from_slice(upstream)?;
	rename(&mut root, "");
	for o in overrides {
		merge(&mut root, o);
	}
	Ok(serde_json::to_vec(&root)?)
}

fn rename(value: &mut Value, key: &str) {
	match value {
		Value::String(s) if s.contains(UPSTREAM_NAME) && !KEEP_PREFIXES.iter().any(|p| key.starts_with(p)) => {
			*s = s.replace(UPSTREAM_NAME, NAME);
		}
		Value::Object(map) => {
			for (k, v) in map.iter_mut() {
				let child = if key.is_empty() {
					k.clone()
				} else {
					format!("{key}.{k}")
				};
				rename(v, &child);
			}
		}
		_ => {}
	}
}

fn merge(base: &mut Value, overrides: Value) {
	match (base, overrides) {
		(Value::Object(base), Value::Object(overrides)) => {
			for (k, v) in overrides {
				match base.get_mut(&k) {
					Some(existing) => merge(existing, v),
					None => {
						base.insert(k, v);
					}
				}
			}
		}
		(base, overrides) => *base = overrides,
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn rebrand(upstream: &str, overrides: Option<&str>) -> Value {
		let overrides = overrides
			.map(|o| serde_json::from_str(o).unwrap())
			.into_iter()
			.collect();
		let out = lang_json(upstream.as_bytes(), overrides).unwrap();
		serde_json::from_slice(&out).unwrap()
	}

	#[test]
	fn renames_but_keeps_credits() {
		let out = rebrand(
			r#"{"WELCOME": {"THANKS": "Thank you for installing WayVR!"}, "DONATE": {"TEAM": "From the WayVR Team"}}"#,
			None,
		);
		assert_eq!(out["WELCOME"]["THANKS"], "Thank you for installing oneVR!");
		assert_eq!(out["DONATE"]["TEAM"], "From the WayVR Team");
	}

	#[test]
	fn merges_overrides() {
		let out = rebrand(
			r#"{"A": {"B": "WayVR", "C": "kept"}}"#,
			Some(r#"{"A": {"B": "Restart oneVR desktop"}, "ONEVR": {"NEW": "New string"}}"#),
		);
		assert_eq!(out["A"]["B"], "Restart oneVR desktop");
		assert_eq!(out["A"]["C"], "kept");
		assert_eq!(out["ONEVR"]["NEW"], "New string");
	}

	#[test]
	fn onevr_strings_fall_back_to_english() {
		let onevr = |p: &str| match p {
			"lang/en.json" => Some(Cow::Borrowed(
				&br#"{"ONEVR": {"A": "English", "B": "English"}, "X": "only English"}"#[..],
			)),
			"lang/de.json" => Some(Cow::Borrowed(&br#"{"ONEVR": {"A": "Deutsch"}}"#[..])),
			_ => None,
		};
		let upstream = |_: &str| Some(Cow::Borrowed(&br#"{"X": "upstream"}"#[..]));
		let de: Value = serde_json::from_slice(&load("lang/de.json", onevr, upstream).unwrap()).unwrap();
		assert_eq!(de["ONEVR"]["A"], "Deutsch");
		assert_eq!(de["ONEVR"]["B"], "English");
		assert_eq!(de["X"], "upstream");
	}

	#[test]
	fn english_only_adds_onevr_strings() {
		for (file, json) in [
			(
				"dash-frontend",
				include_str!("../../dash-frontend/onevr-assets/lang/en.json"),
			),
			("wayvr", include_str!("../../wayvr/onevr-assets/lang/en.json")),
		] {
			let root: Value = serde_json::from_str(json).unwrap();
			for key in root.as_object().unwrap().keys() {
				assert_eq!(
					key, ONEVR_KEY,
					"{file}: rewording {key} changes only English, add a oneVR key instead"
				);
			}
		}
	}

	#[test]
	fn prefers_onevr_assets() {
		let onevr = |p: &str| (p == "a.svg").then(|| Cow::Borrowed(&b"onevr"[..]));
		let upstream = |_: &str| Some(Cow::Borrowed(&b"upstream"[..]));
		assert_eq!(load("a.svg", onevr, upstream).unwrap(), b"onevr");
		assert_eq!(load("b.svg", onevr, upstream).unwrap(), b"upstream");
		assert!(load("lang/en.json", |_| None, |_| None).is_err());
	}
}
