use super::{
    HmdProperties, InteractionProfile, MainAxisType, PoseTransformations, ProfileProperties,
    Property, SkeletalInputBindings, legal_paths, paths::*,
};
use crate::button_mask_from_ids;
use crate::input::BoundPoseType;
use crate::input::legacy::{self, LegacyBindings, button_mask_from_id};
use crate::input::profiles::DynInputPath;
use crate::openxr_data::Hand;
use glam::{EulerRot, Mat4, Quat, Vec3};
use openvr::EVRButtonId;
use std::iter::Iterator;

pub struct Knuckles;

/// Whether Index controllers should present as Vive wands to this app.
///
/// On by default for the games in [`crate::quirks`] and for the names in
/// `$XDG_CONFIG_HOME/xrizer/index_as_vive_apps` (one executable name per line, `#` comments),
/// or for every app with `XRIZER_INDEX_AS_VIVE=1`. `XRIZER_INDEX_AS_VIVE=0` turns it off for
/// every app. Automatic mode is skipped when the app has chosen legacy bindings, so those keep
/// the controller identity they were made for.
pub(crate) fn index_as_vive() -> bool {
    #[cfg(test)]
    let enabled = crate::input::isolated_env_var("XRIZER_INDEX_AS_VIVE")
        .is_some_and(|value| env_enabled(&value));
    #[cfg(not(test))]
    let enabled = {
        struct Auto {
            app: Option<String>,
            explicit: Option<bool>,
            listed: bool,
            builtin: bool,
        }

        static AUTO: std::sync::OnceLock<Auto> = std::sync::OnceLock::new();
        let auto = AUTO.get_or_init(|| {
            if let Some(enabled) = crate::quirks::env_override("XRIZER_INDEX_AS_VIVE") {
                if enabled {
                    log::info!(
                        "XRIZER_INDEX_AS_VIVE is set: Index controllers will present as Vive wands"
                    );
                }
                return Auto {
                    app: None,
                    explicit: Some(enabled),
                    listed: false,
                    builtin: false,
                };
            }

            let apps = crate::input::xrizer_config_dir()
                .and_then(|dir| std::fs::read_to_string(dir.join("index_as_vive_apps")).ok())
                .unwrap_or_default();
            let app = crate::quirks::game_exe_name();
            let listed = app
                .as_deref()
                .is_some_and(|app| index_as_vive_listed(app, &apps));
            let builtin = crate::quirks::get().index_as_vive;
            Auto {
                app,
                explicit: None,
                listed,
                builtin,
            }
        });

        if let Some(enabled) = auto.explicit {
            enabled
        } else {
            let chosen = auto.builtin && !auto.listed && has_chosen_legacy_bindings();
            let enabled = index_as_vive_auto(auto.listed, auto.builtin, chosen);
            if enabled {
                crate::log_unique!(
                    log::Level::Info,
                    "{} is on the Index->Vive list: Index controllers will present as Vive wands",
                    auto.app.as_deref().unwrap_or_default()
                );
            } else if auto.builtin && chosen {
                crate::log_unique!(
                    log::Level::Info,
                    "{} is on the Index->Vive list but has chosen legacy bindings: keeping Index controllers",
                    auto.app.as_deref().unwrap_or_default()
                );
            }
            enabled
        }
    };
    enabled
}

#[cfg(test)]
fn env_enabled(value: &std::ffi::OsStr) -> bool {
    value.to_str().is_some_and(|value| {
        let value = value.trim();
        value == "1" || value.eq_ignore_ascii_case("true")
    })
}

fn index_as_vive_auto(listed: bool, builtin: bool, chosen_legacy_bindings: bool) -> bool {
    listed || (builtin && !chosen_legacy_bindings)
}

#[cfg(not(test))]
fn has_chosen_legacy_bindings() -> bool {
    use std::sync::Mutex;

    // Keyed by the app key: an app that identifies after the first frame gets its per-app
    // chosen bindings checked then.
    static CACHE: Mutex<Option<(Option<String>, bool)>> = Mutex::new(None);
    let app_key = crate::applications::identified_app_key();
    let mut cache = CACHE.lock().unwrap();
    if let Some((cached_key, value)) = &*cache
        && cached_key == &app_key
    {
        return *value;
    }
    let value = has_chosen_legacy_bindings_for(app_key.clone());
    *cache = Some((app_key, value));
    value
}

fn has_chosen_legacy_bindings_for(app_key: Option<String>) -> bool {
    let sources = crate::input::binding_sources::BindingSources::new(
        app_key,
        crate::input::steamvr_bindings::Category::Legacy,
    );
    // Only Knuckles matters here: the automatic mode presents Index controllers as Vive.
    sources
        .file_for(&crate::input::action_manifest::ControllerType::Knuckles)
        .is_some()
}

/// Whether the app with the executable name `app` is in the extra list.
fn index_as_vive_listed(app: &str, extra: &str) -> bool {
    let app = app_stem(app.trim());
    extra
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .any(|listed| app.eq_ignore_ascii_case(app_stem(listed)))
}

fn app_stem(name: &str) -> &str {
    match name.rsplit_once('.') {
        Some((stem, extension)) if extension.eq_ignore_ascii_case("exe") => stem,
        _ => name,
    }
}

/// The headset keeps its identity while the controllers show as Vive wands.
const INDEX_HMD_PROPERTIES: HmdProperties = HmdProperties {
    model: c"Index",
    controller_type: c"indexhmd",
    serial_number: c"LHR-FFFFFFF0",
    render_model_name: c"generic_hmd",
    registered_device_type: c"valve/indexLHR-FFFFFFF0",
    input_profile_path: c"{indexhmd}/input/indexhmd_profile.json",
};

/// What apps see while the controllers present as Vive wands.
static VIVE_COMPAT_PROPERTIES: ProfileProperties = ProfileProperties {
    model: Property::BothHands(c"Vive. MV"),
    openvr_controller_type: c"vive_controller",
    render_model_name: Property::BothHands(c"vr_controller_vive_1_5"),
    main_axis: MainAxisType::Trackpad,
    // Keep Index's serials, so the devices stay unique and tracked as lighthouse ones.
    registered_device_type: Property::PerHand {
        left: c"htc/vive_controllerLHR-FFFFFFF1",
        right: c"htc/vive_controllerLHR-FFFFFFF2",
    },
    serial_number: Property::PerHand {
        left: c"LHR-FFFFFFF1",
        right: c"LHR-FFFFFFF2",
    },
    tracking_system_name: c"lighthouse",
    manufacturer_name: c"HTC",
    input_profile_path: c"{htc}/input/vive_controller_profile.json",
    legacy_buttons_mask: button_mask_from_ids!(
        EVRButtonId::System,
        EVRButtonId::ApplicationMenu,
        EVRButtonId::Grip,
        EVRButtonId::A,
        EVRButtonId::Axis0,
        EVRButtonId::Axis1
    ),
    hmd: INDEX_HMD_PROPERTIES,
};

static DEVICE_PROPERTIES: ProfileProperties = ProfileProperties {
    model: Property::PerHand {
        left: c"Knuckles Left",
        right: c"Knuckles Right",
    },
    openvr_controller_type: c"knuckles",
    render_model_name: Property::PerHand {
        left: c"{indexcontroller}valve_controller_knu_1_0_left",
        right: c"{indexcontroller}valve_controller_knu_1_0_right",
    },
    main_axis: MainAxisType::Thumbstick,
    registered_device_type: Property::PerHand {
        left: c"valve/index_controllerLHR-FFFFFFF1",
        right: c"valve/index_controllerLHR-FFFFFFF2",
    },
    serial_number: Property::PerHand {
        left: c"LHR-FFFFFFF1",
        right: c"LHR-FFFFFFF2",
    },
    tracking_system_name: c"lighthouse",
    manufacturer_name: c"Valve",
    input_profile_path: c"{indexcontroller}/input/index_controller_profile.json",
    // Like SteamVR, without A, which is reported as Grip.
    legacy_buttons_mask: button_mask_from_ids!(
        EVRButtonId::System,
        EVRButtonId::ApplicationMenu,
        EVRButtonId::Grip,
        EVRButtonId::Axis0,
        EVRButtonId::Axis1,
        EVRButtonId::Axis2
    ),
    hmd: INDEX_HMD_PROPERTIES,
};

/// The properties controllers use while they present as Vive wands.
pub(in crate::input) fn compat_properties() -> &'static ProfileProperties {
    &VIVE_COMPAT_PROPERTIES
}

impl InteractionProfile for Knuckles {
    // The trackpad clicks by force, and a dpad on it plays a tick when a direction engages.
    const USE_FORCE_DPAD: bool = true;
    // The finger curl sensors measure the fingers only partially.
    const SKELETAL_TRACKING_LEVEL: Option<openvr::EVRSkeletalTrackingLevel> =
        Some(openvr::EVRSkeletalTrackingLevel::Partial);
    const SENSES_FINGERS: bool = true;
    // From legacy_bindings_index_controller.json
    const LEGACY_TRIGGER_CLICK: Option<(f32, f32)> = Some((0.55, 0.5));
    type LegalPaths = legal_paths![
        Both::<
            (System, Click),
            (System, Touch),
            (A, Click),
            (A, Touch),
            (B, Click),
            (B, Touch),
            (Trigger, Click),
            (Trigger, Touch),
            (Trigger, Value),
            (Squeeze, Value),
            (Squeeze, Force),
            (Thumbstick, Click),
            (Thumbstick, Touch),
            (Thumbstick, ()),
            (Trackpad, Force),
            (Trackpad, Touch),
            (Trackpad, ()),
        >
    ];

    fn profile_path() -> &'static str {
        "/interaction_profiles/valve/index_controller"
    }
    fn has_required_extensions(_: &openxr::ExtensionSet) -> bool {
        true
    }
    fn properties() -> &'static ProfileProperties {
        &DEVICE_PROPERTIES
    }
    fn translate_path(path: DynInputPath) -> Option<DynInputPath> {
        match path {
            p @ DynInputPath {
                subpath: DynSubpath::Trackpad,
                component: Some(DynComponent::Click),
                ..
            } => Some(p.with_component(DynComponent::Force)),
            p @ DynInputPath {
                subpath: DynSubpath::Squeeze,
                component: Some(DynComponent::Touch),
                ..
            } => Some(p.with_component(DynComponent::Value)),
            _ => None,
        }
    }

    fn legacy_bindings(c: &super::InputToXrPath<Self>) -> LegacyBindings {
        if index_as_vive() {
            // A Vive wand: the stick is its trackpad, its touch is emulated from the
            // deflection in `read_hand_state`, and A is a face button of its own instead of
            // another grip.
            return LegacyBindings {
                extra: legacy::Bindings {
                    grip_pose: c.pose(),
                },
                app_menu: c.leftright::<B, Click, _, _>(),
                app_menu_touch: vec![],
                a: c.leftright::<A, Click, _, _>(),
                a_touch: vec![],
                trigger: c.leftright::<Trigger, Value, _, _>(),
                trigger_click: c.leftright::<Trigger, Click, _, _>(),
                trigger_touch: vec![],
                squeeze: c.leftright::<Squeeze, Force, _, _>(),
                squeeze_click: c.leftright::<Squeeze, Force, _, _>(),
                grip_click: vec![],
                axis2_touch: vec![],
                grip_touch: vec![],
                main_xy: c.leftright::<Thumbstick, (), _, _>(),
                main_xy_click: c.leftright::<Thumbstick, Click, _, _>(),
                main_xy_touch: vec![],
                main_xy_deflect: vec![],
                haptic: c.haptics(),
            };
        }
        // Follows SteamVR's legacy_bindings_index_controller.json
        LegacyBindings {
            extra: legacy::Bindings {
                grip_pose: c.pose(),
            },
            app_menu: c.leftright::<B, Click, _, _>(),
            app_menu_touch: c.leftright::<B, Touch, _, _>(),
            a: vec![],
            a_touch: vec![],
            trigger: c.leftright::<Trigger, Value, _, _>(),
            trigger_click: c.leftright::<Trigger, Click, _, _>(),
            trigger_touch: c.leftright::<Trigger, Touch, _, _>(),
            // squeeze/value is the capacitive finger curl, which is already high when the
            // controller is merely held, so the grip is pressed through the force sensor.
            squeeze: c.leftright::<Squeeze, Force, _, _>(),
            squeeze_click: c.leftright::<Squeeze, Force, _, _>(),
            grip_click: c.leftright::<A, Click, _, _>(),
            axis2_touch: vec![],
            grip_touch: [
                c.leftright::<A, Touch, _, _>(),
                c.leftright::<Squeeze, Value, _, _>(),
            ]
            .concat(),
            // The stick and the trackpad both drive Axis0.
            main_xy: [
                c.leftright::<Thumbstick, (), _, _>(),
                c.leftright::<Trackpad, (), _, _>(),
            ]
            .concat(),
            main_xy_click: [
                c.leftright::<Thumbstick, Click, _, _>(),
                c.leftright::<Trackpad, Force, _, _>(),
            ]
            .concat(),
            main_xy_touch: [
                c.leftright::<Thumbstick, Touch, _, _>(),
                c.leftright::<Trackpad, Touch, _, _>(),
            ]
            .concat(),
            main_xy_deflect: c.leftright::<Thumbstick, (), _, _>(),
            haptic: c.haptics(),
        }
    }

    fn skeletal_input_bindings(c: &super::InputToXrPath<Self>) -> SkeletalInputBindings {
        SkeletalInputBindings {
            thumb_touch: c
                .leftright::<Thumbstick, Touch, _, _>()
                .into_iter()
                .chain(c.leftright::<Trackpad, Touch, _, _>())
                .chain(c.leftright::<A, Touch, _, _>())
                .chain(c.leftright::<B, Touch, _, _>())
                .collect(),
            index_touch: c.leftright::<Trigger, Touch, _, _>(),
            index_curl: c.leftright::<Trigger, Value, _, _>(),
            rest_curl: c.leftright::<Squeeze, Value, _, _>(),
        }
    }

    fn offset_grip_pose(hand: Hand) -> Mat4 {
        super::offset_grip_pose_from_pose_type::<Knuckles>(hand, BoundPoseType::Grip)
    }

    /// Derived from `SteamVR/drivers/indexcontroller/resources/rendermodels/valve_controller_knu_1_0*/*.json`
    fn pose_transformation(pose: BoundPoseType) -> Option<PoseTransformations> {
        match pose {
            BoundPoseType::Raw => None,
            // All 0 so skip transformation
            BoundPoseType::Gdc2015 => None,
            // Not present in rendermodels
            BoundPoseType::OpenxrAim => None,
            // Not present in rendermodels
            BoundPoseType::OpenxrGrip => None,
            BoundPoseType::Tip => Some(PoseTransformations {
                left_hand: Mat4::from_rotation_translation(
                    Quat::from_euler(
                        EulerRot::ZYX,
                        0_f32.to_radians(),
                        -5.0_f32.to_radians(),
                        -40.0_f32.to_radians(),
                    ),
                    Vec3::new(0.006, -0.015, 0.02),
                ),
                right_hand: Mat4::from_rotation_translation(
                    Quat::from_euler(
                        EulerRot::ZYX,
                        0_f32.to_radians(),
                        5.0_f32.to_radians(),
                        -40.0_f32.to_radians(),
                    ),
                    Vec3::new(-0.006, -0.015, 0.02),
                ),
            }),
            BoundPoseType::Base => Some(PoseTransformations {
                left_hand: Mat4::from_rotation_translation(
                    Quat::from_euler(
                        EulerRot::ZYX,
                        7.081_f32.to_radians(),
                        -0.427_f32.to_radians(),
                        -155.4_f32.to_radians(),
                    ),
                    Vec3::new(0.004758, -0.037977, 0.200466),
                ),
                right_hand: Mat4::from_rotation_translation(
                    Quat::from_euler(
                        EulerRot::ZYX,
                        -7.081_f32.to_radians(),
                        -0.427_f32.to_radians(),
                        -155.4_f32.to_radians(),
                    ),
                    Vec3::new(-0.004758, -0.037977, 0.200466),
                ),
            }),
            BoundPoseType::Handgrip => Some(PoseTransformations {
                left_hand: Mat4::from_rotation_translation(
                    Quat::from_euler(
                        EulerRot::ZYX,
                        0.303_f32.to_radians(),
                        -2.071_f32.to_radians(),
                        15.392_f32.to_radians(),
                    ),
                    Vec3::new(-0.003851, 0.003715, 0.075948),
                ),
                right_hand: Mat4::from_rotation_translation(
                    Quat::from_euler(
                        EulerRot::ZYX,
                        -0.303_f32.to_radians(),
                        2.071_f32.to_radians(),
                        15.392_f32.to_radians(),
                    ),
                    Vec3::new(0.003851, 0.003715, 0.075948),
                ),
            }),
            BoundPoseType::Grip => Some(PoseTransformations {
                left_hand: Mat4::from_rotation_translation(
                    Quat::from_euler(
                        EulerRot::ZYX,
                        0.303_f32.to_radians(),
                        -2.071_f32.to_radians(),
                        15.392_f32.to_radians(),
                    ),
                    Vec3::new(0.0, -0.015, 0.13),
                ),
                right_hand: Mat4::from_rotation_translation(
                    Quat::from_euler(
                        EulerRot::ZYX,
                        -0.303_f32.to_radians(),
                        2.071_f32.to_radians(),
                        15.392_f32.to_radians(),
                    ),
                    Vec3::new(0.0, -0.015, 0.13),
                ),
            }),
            BoundPoseType::OpenxrHandmodel => Some(PoseTransformations {
                left_hand: Mat4::from_rotation_translation(
                    Quat::from_euler(
                        EulerRot::ZYX,
                        0_f32.to_radians(),
                        -5_f32.to_radians(),
                        -40_f32.to_radians(),
                    ),
                    Vec3::new(-0.015, -0.015, 0.13),
                ),
                right_hand: Mat4::from_rotation_translation(
                    Quat::from_euler(
                        EulerRot::ZYX,
                        0_f32.to_radians(),
                        5_f32.to_radians(),
                        -40_f32.to_radians(),
                    ),
                    Vec3::new(0.015, -0.015, 0.13),
                ),
            }),
            BoundPoseType::OpenxrPinch => Some(PoseTransformations {
                left_hand: Mat4::from_rotation_translation(
                    Quat::from_euler(
                        EulerRot::ZYX,
                        35_f32.to_radians(),
                        -15_f32.to_radians(),
                        -70_f32.to_radians(),
                    ),
                    Vec3::new(0.05, 0.006, 0.0675),
                ),
                right_hand: Mat4::from_rotation_translation(
                    Quat::from_euler(
                        EulerRot::ZYX,
                        -35_f32.to_radians(),
                        15_f32.to_radians(),
                        -70_f32.to_radians(),
                    ),
                    Vec3::new(-0.05, 0.006, 0.0675),
                ),
            }),
            BoundPoseType::OpenxrPoke => Some(PoseTransformations {
                left_hand: Mat4::from_rotation_translation(
                    Quat::from_euler(
                        EulerRot::ZYX,
                        0_f32.to_radians(),
                        0_f32.to_radians(),
                        -60_f32.to_radians(),
                    ),
                    Vec3::new(-0.04, -0.0575, 0.0039),
                ),
                right_hand: Mat4::from_rotation_translation(
                    Quat::from_euler(
                        EulerRot::ZYX,
                        0_f32.to_radians(),
                        0_f32.to_radians(),
                        -60_f32.to_radians(),
                    ),
                    Vec3::new(0.04, -0.0575, 0.0039),
                ),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{InteractionProfile, Knuckles};
    use crate::input::{ActionData, custom_bindings::BoolBindingType, tests::Fixture};
    use openxr as xr;

    #[test]
    fn verify_bindings() {
        let f = Fixture::new();
        f.load_actions(c"actions.json");

        let path = Knuckles::profile_path();
        f.verify_bindings::<bool>(
            path,
            c"/actions/set1/in/boolact",
            [
                "/user/hand/left/input/a/click".into(),
                "/user/hand/right/input/a/click".into(),
                "/user/hand/left/input/b/click".into(),
                "/user/hand/right/input/b/click".into(),
                "/user/hand/left/input/trigger/touch".into(),
                "/user/hand/right/input/trigger/touch".into(),
                "/user/hand/left/input/thumbstick/click".into(),
                "/user/hand/right/input/thumbstick/click".into(),
                "/user/hand/left/input/thumbstick/touch".into(),
                "/user/hand/right/input/thumbstick/touch".into(),
                "/user/hand/right/input/trackpad/touch".into(),
                "/user/hand/left/input/trackpad/force".into(),
                "/user/hand/right/input/trackpad/force".into(),
            ],
        );

        f.verify_threshold_bindings(
            path,
            c"/actions/set1/in/boolact",
            [
                "/user/hand/left/input/trigger/value".into(),
                "/user/hand/right/input/trigger/value".into(),
                "/user/hand/left/input/squeeze/value".into(),
            ],
        );

        f.verify_bindings::<f32>(
            path,
            c"/user/hand/left/input/trackpad/force-/actions/set1",
            ["/user/hand/left/input/trackpad/force".into()],
        );

        let handle = f.get_action_handle(c"/actions/set1/in/boolact");
        let data = f.input.openxr.session_data.get();
        let actions = data.input_data.get_loaded_actions().unwrap();
        let ActionData::Bool(_) = actions.try_get_action(handle).unwrap() else {
            panic!("no");
        };

        let grab_force =
            f.custom_binding_action(handle, path, |ty| matches!(ty, BoolBindingType::Grab(_)));
        let p = f.input.openxr.instance.string_to_path(path).unwrap();
        let suggested = fakexr::get_suggested_bindings(grab_force, p);
        assert!(suggested.contains(&"/user/hand/right/input/squeeze/force".to_string()));

        f.verify_bindings::<f32>(
            path,
            c"/actions/set1/in/vec1act",
            [
                "/user/hand/left/input/trigger/value".into(),
                "/user/hand/right/input/trigger/value".into(),
                "/user/hand/left/input/squeeze/force".into(),
                "/user/hand/right/input/squeeze/value".into(),
            ],
        );

        f.verify_bindings::<xr::Vector2f>(
            path,
            c"/actions/set1/in/vec2act",
            [
                "/user/hand/left/input/trackpad".into(),
                "/user/hand/right/input/trackpad".into(),
                "/user/hand/left/input/thumbstick".into(),
                "/user/hand/right/input/thumbstick".into(),
            ],
        );

        // Scrolling reads the stick through an action of its own.
        let scrollact =
            f.get_action::<xr::Vector2f>(f.get_action_handle(c"/actions/set1/in/scrollact"));
        assert!(fakexr::check_no_suggested_bindings(scrollact, p));
        f.verify_bindings::<xr::Vector2f>(
            path,
            c"/user/hand/left/input/thumbstick-/actions/set1",
            ["/user/hand/left/input/thumbstick".into()],
        );

        f.verify_bindings::<xr::Haptic>(
            path,
            c"/actions/set1/in/vib",
            [
                "/user/hand/left/output/haptic".into(),
                "/user/hand/right/output/haptic".into(),
            ],
        );
    }

    #[test]
    fn index_as_vive_listed_apps() {
        assert!(super::index_as_vive_listed("MyGame.EXE", "mygame\n"));
        assert!(super::index_as_vive_listed(
            "MyGame",
            "# a comment\n\n mygame.exe \n"
        ));
        assert!(!super::index_as_vive_listed("MyGame", "# MyGame\n"));
        assert!(!super::index_as_vive_listed("SkyrimVR.exe", ""));
    }

    #[test]
    fn index_as_vive_override() {
        use std::ffi::OsStr;
        assert!(super::env_enabled(OsStr::new("1")));
        assert!(super::env_enabled(OsStr::new("true")));
        assert!(super::env_enabled(OsStr::new("TRUE")));
        assert!(!super::env_enabled(OsStr::new("0")));
        assert!(!super::env_enabled(OsStr::new("")));
    }

    #[test]
    fn index_as_vive_auto_rule() {
        // The explicit app list wins; chosen bindings only disable the built-in list.
        assert!(super::index_as_vive_auto(true, false, true));
        assert!(super::index_as_vive_auto(false, true, false));
        assert!(!super::index_as_vive_auto(false, true, true));
        assert!(!super::index_as_vive_auto(false, false, false));
    }

    #[test]
    fn chosen_legacy_bindings_disable_auto() {
        // A legacy binding for another controller doesn't replace the Index identity.
        crate::input::tests::set_test_env(
            "XRIZER_CUSTOM_BINDINGS_DIR",
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/input_data/legacy_mirrored"
            ),
        );
        assert!(!super::has_chosen_legacy_bindings_for(None));

        crate::input::tests::set_test_env(
            "XRIZER_CUSTOM_BINDINGS_DIR",
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/input_data/legacy_index_as_vive"
            ),
        );
        assert!(super::has_chosen_legacy_bindings_for(None));

        crate::input::tests::set_test_env(
            "XRIZER_CUSTOM_BINDINGS_DIR",
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/input_data/custom_bindings"
            ),
        );
        assert!(!super::has_chosen_legacy_bindings_for(None));
    }

    #[test]
    fn chosen_steamvr_legacy_bindings_disable_auto() {
        use crate::input::tests::{FakeSteam, input_data};
        let steam = FakeSteam::new("index_as_vive_gate");
        let url = format!(
            "file://{}",
            input_data("legacy_index_as_vive/knuckles.json")
        );
        steam.choose(&[("knuckles_250820_CurrentURL_legacy", &url)]);
        assert!(super::has_chosen_legacy_bindings_for(Some(
            FakeSteam::APP_KEY.to_owned()
        )));
    }
}
