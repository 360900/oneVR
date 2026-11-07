use std::{marker::PhantomData, rc::Rc};

use wgui::{
	assets::AssetPathRef,
	components::button::ComponentButton,
	globals::WguiGlobals,
	layout::{Layout, WidgetID},
	parser::{Fetchable, ParseDocumentParams, ParserState, TemplateParams},
	task::Tasks,
};

use crate::{
	frontend::{Frontend, FrontendTask, FrontendTasks},
	tab::{Tab, TabType},
};

#[derive(Clone)]
#[allow(clippy::enum_variant_names)]
enum Task {
	SetPage(u8),
	SetPageNext,
	SetPagePrev,
}

pub struct TabWelcome<T> {
	#[allow(dead_code)]
	pub state: ParserState,
	marker: PhantomData<T>,
	tasks: Tasks<Task>,
	current_page: u8,
	id_pips: WidgetID,
	id_content: WidgetID,
	frontend_tasks: FrontendTasks,

	state_tab: Option<ParserState>,
}

/// oneVR: the pages in order, in `gui/tab/`; oneVR's hand tracking page follows the laser colors
const PAGES: [&str; 7] = [
	"welcome_page_0",
	"welcome_page_1",
	"welcome_page_2",
	"welcome_page_3",
	"welcome_page_hands",
	"welcome_page_4",
	"welcome_page_5",
];
const PAGE_COUNT: u8 = PAGES.len() as u8;

impl<T> Tab<T> for TabWelcome<T> {
	fn get_type(&self) -> TabType {
		TabType::Welcome
	}

	fn update(&mut self, frontend: &mut Frontend<T>, _time_ms: u32, _user_data: &mut T) -> anyhow::Result<()> {
		for task in self.tasks.drain() {
			match task {
				Task::SetPage(page_num) => {
					self.set_page(&mut frontend.layout, page_num)?;
				}
				Task::SetPageNext => {
					if self.current_page < PAGE_COUNT - 1 {
						self.tasks.push(Task::SetPage(self.current_page + 1));
					}
				}
				Task::SetPagePrev => {
					if self.current_page > 0 {
						self.tasks.push(Task::SetPage(self.current_page - 1));
					}
				}
			}
		}

		Ok(())
	}
}

fn doc_params(globals: &WguiGlobals) -> ParseDocumentParams<'_> {
	ParseDocumentParams {
		globals: globals.clone(),
		path: AssetPathRef::BuiltIn("gui/tab/welcome.xml"),
		extra: Default::default(),
	}
}

impl<T> TabWelcome<T> {
	pub fn new(frontend: &mut Frontend<T>, parent_id: WidgetID, _data: &mut T) -> anyhow::Result<Self> {
		let state = wgui::parser::parse_from_assets(&doc_params(&frontend.globals), &mut frontend.layout, parent_id)?;

		let tasks = Tasks::<Task>::new();

		let btn_prev = state.fetch_component_as::<ComponentButton>("btn_prev")?;
		let btn_next = state.fetch_component_as::<ComponentButton>("btn_next")?;

		tasks.handle_button(&btn_prev, Task::SetPagePrev);
		tasks.handle_button(&btn_next, Task::SetPageNext);

		let id_pips = state.get_widget_id("pips")?;
		let id_content = state.get_widget_id("content")?;

		tasks.push(Task::SetPage(0));

		Ok(Self {
			state,
			marker: PhantomData,
			current_page: 0,
			id_pips,
			id_content,
			tasks,
			state_tab: None,
			frontend_tasks: frontend.tasks.clone(),
		})
	}

	fn refresh_pips(&mut self, layout: &mut Layout) -> anyhow::Result<()> {
		layout.remove_children(self.id_pips);

		let globals = layout.state.globals.clone();

		for i in 0..PAGE_COUNT {
			let mut params = TemplateParams::new();
			let is_selected = i == self.current_page;
			params.insert("COLOR", if is_selected { "accent" } else { "fill_strong" });

			self
				.state
				.instantiate_template(&doc_params(&globals), "Pip", layout, self.id_pips, params)?
		}

		Ok(())
	}

	fn fill_page(&mut self, layout: &mut Layout) -> anyhow::Result<()> {
		layout.remove_children(self.id_content);

		let globals = layout.state.globals.clone();

		let state = wgui::parser::parse_from_assets(
			&ParseDocumentParams {
				globals,
				path: AssetPathRef::BuiltIn(&format!("gui/tab/{}.xml", PAGES[usize::from(self.current_page)])),
				extra: Default::default(),
			},
			layout,
			self.id_content,
		)?;

		if let Ok(btn) = state.fetch_component_as::<ComponentButton>("btn_home_screen") {
			btn.on_click({
				let tasks = self.frontend_tasks.clone();
				Rc::new(move |_, _| {
					tasks.push(FrontendTask::SetTab(TabType::Home));
					tasks.push(FrontendTask::MarkTutorialGraduated);
					Ok(())
				})
			});
		}

		if let Ok(btn) = state.fetch_component_as::<ComponentButton>("btn_wayvr_org") {
			self
				.frontend_tasks
				.handle_button(&btn, FrontendTask::OpenURL(Rc::from("https://wayvr.org")));
		}

		self.state_tab = Some(state);

		Ok(())
	}

	fn set_page(&mut self, layout: &mut Layout, page_num: u8) -> anyhow::Result<()> {
		self.current_page = page_num;

		self.refresh_pips(layout)?;
		self.fill_page(layout)?;

		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use std::{rc::Rc, sync::Mutex};

	use wgui::{
		assets::{AssetPathRef, AssetProvider},
		i18n::Locale,
		parser::ParseDocumentParams,
		theme::WguiTheme,
	};
	use wlx_common::{dash_interface_emulated::DashInterfaceEmulated, locale::WayVRLangProvider};

	use super::PAGES;
	use crate::{
		assets::Asset,
		frontend::{Frontend, InitParams},
	};

	/// The parser's warnings, which it logs for an unknown tag or attribute instead of failing
	static WARNINGS: Mutex<Vec<String>> = Mutex::new(Vec::new());

	struct Warnings;

	impl log::Log for Warnings {
		fn enabled(&self, metadata: &log::Metadata) -> bool {
			metadata.level() <= log::Level::Warn
		}

		fn log(&self, record: &log::Record) {
			if self.enabled(record.metadata()) {
				WARNINGS.lock().unwrap().push(record.args().to_string());
			}
		}

		fn flush(&self) {}
	}

	/// oneVR: every page of the tutorial builds, with oneVR's among upstream's, and oneVR's
	/// without a warning
	#[test]
	fn pages_build() {
		let _ = log::set_logger(&Warnings);
		log::set_max_level(log::LevelFilter::Warn);
		let lang_provider = WayVRLangProvider::default();
		let locale = Locale::from_env(&lang_provider);
		let mut frontend = Frontend::new(InitParams {
			interface: Box::new(DashInterfaceEmulated::new_headless(&locale, None)),
			show_welcome: true,
			has_monado: true,
			lang_provider: &lang_provider,
			theme: Rc::new(WguiTheme::default()),
			executor: wlx_common::async_executor::create_local(),
		})
		.unwrap();
		let parent = frontend.layout.content_root_widget;
		for page in PAGES {
			let path = format!("gui/tab/{page}.xml");
			let params = ParseDocumentParams {
				globals: frontend.globals.clone(),
				path: AssetPathRef::BuiltIn(&path),
				extra: Default::default(),
			};
			if let Err(e) = wgui::parser::parse_from_assets(&params, &mut frontend.layout, parent) {
				panic!("{page}: {e:?}");
			}
		}
		let warnings = WARNINGS.lock().unwrap();
		let own: Vec<_> = warnings.iter().filter(|w| w.contains("welcome_page_hands")).collect();
		assert!(own.is_empty(), "{own:#?}");
	}

	/// oneVR: the hand tracking page's strings are in oneVR's English
	#[test]
	fn hand_page_strings() {
		let page = String::from_utf8(Asset.load_from_path("gui/tab/welcome_page_hands.xml").unwrap()).unwrap();
		let en: serde_json::Value = serde_json::from_slice(&Asset.load_from_path("lang/en.json").unwrap()).unwrap();
		let mut keys = 0;
		for (start, _) in page.match_indices("\"ONEVR.") {
			let key = &page[start + 1..];
			let key = &key[..key.find('"').unwrap()];
			let value = key.split('.').try_fold(&en, |v, k| v.get(k));
			assert!(value.is_some_and(serde_json::Value::is_string), "{key} is missing");
			keys += 1;
		}
		assert_eq!(keys, 9);
	}
}
