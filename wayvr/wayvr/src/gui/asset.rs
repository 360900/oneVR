#[derive(rust_embed::Embed)]
#[folder = "src/assets/"]
pub struct GuiAsset;

/// oneVR's replacements and additions, looked up before `GuiAsset`.
#[derive(rust_embed::Embed)]
#[folder = "onevr-assets/"]
struct OneVrAsset;

impl wgui::assets::AssetProvider for GuiAsset {
    fn load_from_path(&mut self, path: &str) -> anyhow::Result<Vec<u8>> {
        onevr_common::rebrand::load(
            path,
            |p| OneVrAsset::get(p).map(|f| f.data),
            |p| Self::get(p).map(|f| f.data),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wgui::assets::AssetProvider;

    #[test]
    fn onevr_branding() {
        for file in GuiAsset::iter().filter(|f| f.starts_with("lang/") && f.ends_with(".json")) {
            let data = GuiAsset.load_from_path(&file).unwrap();
            assert!(
                !String::from_utf8_lossy(&data).contains("WayVR"),
                "{file} still says WayVR"
            );
        }
        let icon = GuiAsset
            .load_from_path("watch/wayvr_dashboard_mono.svg")
            .unwrap();
        assert_eq!(
            icon,
            OneVrAsset::get("watch/wayvr_dashboard_mono.svg")
                .unwrap()
                .data
                .as_ref()
        );
    }

    /// The toasts of failed oneVR requests say what failed and why, with onevr-common's keys and
    /// the dashboard's words
    #[test]
    fn onevr_failure_strings() {
        let en: serde_json::Value =
            serde_json::from_slice(&GuiAsset.load_from_path("lang/en.json").unwrap()).unwrap();
        let source = include_str!("../../../onevr-common/src/state.rs");
        let mut keys = 0;
        for prefix in ["\"ONEVR.FAILED.", "\"ONEVR.REASON."] {
            for (start, _) in source.match_indices(prefix) {
                let key = &source[start + 1..];
                let key = &key[..key.find('"').unwrap()];
                let value = key.split('.').try_fold(&en, |v, k| v.get(k));
                assert!(
                    value.is_some_and(serde_json::Value::is_string),
                    "{key} is missing from en.json"
                );
                keys += 1;
            }
        }
        assert!(keys > 10, "found only {keys} keys");
        let dash = include_str!("../../../dash-frontend/onevr-assets/lang/en.json");
        let dash: serde_json::Value = serde_json::from_str(dash).unwrap();
        for block in ["FAILED", "REASON"] {
            assert_eq!(
                en["ONEVR"][block], dash["ONEVR"][block],
                "ONEVR.{block} differs from dash-frontend's en.json, which onevr-ui shows"
            );
        }
    }
}
