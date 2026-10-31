use std::io::Read;

use cosmic_text::PlatformFallback;
use flate2::read::GzDecoder;
use parking_lot::Mutex;

use crate::{assets_internal::AssetInternal, i18n::Locale};

/// oneDesign typeface, bundled so every surface renders the same
pub const DEFAULT_FAMILY: &str = "Inter";

const BUNDLED_FONTS: &[&str] = &[
	"fonts/Inter-Light.ttf.gz",
	"fonts/Inter-Regular.ttf.gz",
	"fonts/Inter-SemiBold.ttf.gz",
	"fonts/Inter-Bold.ttf.gz",
	"fonts/Inter-ExtraBold.ttf.gz",
];

fn load_bundled_fonts(db: &mut cosmic_text::fontdb::Database) {
	for path in BUNDLED_FONTS {
		let Some(file) = AssetInternal::get(path) else {
			log::error!("bundled font {path} not found");
			continue;
		};
		let mut data = Vec::new();
		if let Err(e) = GzDecoder::new(file.data.as_ref()).read_to_end(&mut data) {
			log::error!("failed to decompress bundled font {path}: {e}");
			continue;
		}
		db.load_font_data(data);
	}
}

#[derive(Default)]
pub struct WguiFontConfig<'a> {
	pub binaries: Vec<&'a [u8]>,
	pub family_name_sans_serif: &'a str,
	pub family_name_serif: &'a str,
	pub family_name_monospace: &'a str,
}

pub struct WguiFontSystem {
	pub system: Mutex<cosmic_text::FontSystem>,
}

impl WguiFontSystem {
	pub fn new(config: &WguiFontConfig, locale: &Locale) -> Self {
		let mut db = cosmic_text::fontdb::Database::new();

		// needed for fallback
		db.load_system_fonts();
		load_bundled_fonts(&mut db);

		for binary in &config.binaries {
			// binary data is copied and preserved here
			db.load_font_data(binary.to_vec());
		}

		db.set_sans_serif_family(if config.family_name_sans_serif.is_empty() {
			DEFAULT_FAMILY
		} else {
			config.family_name_sans_serif
		});

		db.set_serif_family(if config.family_name_serif.is_empty() {
			DEFAULT_FAMILY
		} else {
			config.family_name_serif
		});

		if !config.family_name_monospace.is_empty() {
			db.set_monospace_family(config.family_name_monospace);
		}

		let system = cosmic_text::FontSystem::new_with_locale_and_db_and_fallback(
			locale.get_matched().to_owned(),
			db,
			PlatformFallback,
		);

		Self {
			system: Mutex::new(system),
		}
	}
}
