#[derive(Clone)]
pub struct WguiTheme {
	pub animation_mult: f32,
}

impl Default for WguiTheme {
	fn default() -> Self {
		Self { animation_mult: 1.0 }
	}
}
