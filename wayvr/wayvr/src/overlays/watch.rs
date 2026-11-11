use crate::{
    gui::{
        panel::{
            GuiPanel, NewGuiPanelParams, apply_custom_command, device_list::DeviceList,
            overlay_list::OverlayList, set_list::SetList,
        },
        timer::GuiTimer,
    },
    state::AppState,
    windowing::{Z_ORDER_WATCH, backend::OverlayEventData, window::OverlayWindowConfig},
};
use glam::{Affine3A, Mat3A, Quat, Vec3, Vec3A, vec3};
use std::time::Duration;
use wgui::{
    assets::AssetPathRef,
    components::button::ComponentButton,
    parser::{Fetchable, ParseDocumentParams},
};
use wlx_common::{
    common::LeftRight,
    windowing::{OverlayWindowState, Positioning},
};

pub const WATCH_NAME: &str = "watch";
pub const WATCH_POS: Vec3 = vec3(-0.03, -0.01, 0.125);
/// oneVR: the watch's center from a tracked hand's wrist joint, in its space before the tilt: on the
/// back of the forearm, just past the hand
const WRIST_WATCH_POS: Vec3A = Vec3A::new(0.0, 0.025, 0.05);
/// oneVR: how far the watch turns around the forearm toward the thumb, which faces the user
const WRIST_WATCH_TILT: f32 = std::f32::consts::FRAC_PI_4;
pub const WATCH_ROT: Quat = Quat::from_xyzw(-0.707_106_6, 0.000_796_361_8, 0.707_106_6, 0.0);
/// oneVR: a toast shown on the watch lies on its face, this far in front of it
const WATCH_TOAST_LIFT: Vec3 = vec3(0.0, 0.0, 0.005);

#[derive(Default)]
struct WatchState {
    device_list: DeviceList,
    overlay_list: OverlayList,
    set_list: SetList,
    clock_12h: bool,
}

pub fn create_watch(app: &mut AppState) -> anyhow::Result<OverlayWindowConfig> {
    let state = WatchState {
        clock_12h: app.session.config.clock_12h,
        ..Default::default()
    };
    let watch_xml = "gui/watch.xml";

    let mut panel =
        GuiPanel::new_from_template(app, watch_xml, state, NewGuiPanelParams::default())?;

    sets_or_overlays(&mut panel, app);

    let doc_params = ParseDocumentParams {
        globals: panel.layout.state.globals.clone(),
        path: AssetPathRef::FileOrBuiltIn {
            path: watch_xml,
            fallback: None,
        },
        extra: panel.doc_extra.take().unwrap_or_default(),
    };

    panel.on_notify = Some(Box::new({
        let name = WATCH_NAME;
        move |panel, app, event_data| {
            let mut elems_changed = panel.state.overlay_list.on_notify(
                &mut panel.layout,
                &mut panel.parser_state,
                &event_data,
                &doc_params,
            )?;

            elems_changed |= panel.state.set_list.on_notify(
                &mut panel.layout,
                &mut panel.parser_state,
                &event_data,
                &doc_params,
            )?;

            elems_changed |= panel.state.device_list.on_notify(
                app,
                &mut panel.layout,
                &mut panel.parser_state,
                &event_data,
                &doc_params,
            )?;

            match event_data {
                OverlayEventData::EditModeChanged(edit_mode) => {
                    if let Ok(btn_edit_mode) = panel
                        .parser_state
                        .fetch_component_as::<ComponentButton>("btn_edit_mode")
                    {
                        btn_edit_mode.set_sticky_state(&mut panel.layout.common(), edit_mode);
                    }
                }
                OverlayEventData::SettingsChanged => {
                    panel.layout.mark_redraw();
                    sets_or_overlays(panel, app);

                    if app.session.config.clock_12h != panel.state.clock_12h {
                        panel.state.clock_12h = app.session.config.clock_12h;

                        let clock_root = panel.parser_state.get_widget_id("clock_root")?;
                        panel.layout.remove_children(clock_root);

                        panel.parser_state.instantiate_template(
                            &doc_params,
                            "Clock",
                            &mut panel.layout,
                            clock_root,
                            Default::default(),
                        )?;

                        elems_changed = true;
                    }
                }
                OverlayEventData::CustomCommand { element, command } => {
                    if let Err(e) = apply_custom_command(panel, app, &element, &command) {
                        log::warn!("Could not apply {command:?} on {name}/{element}: {e:?}");
                    } else {
                        elems_changed = true;
                    }
                }
                _ => {}
            }

            if elems_changed {
                panel.process_custom_elems(app);
            }

            Ok(())
        }
    }));

    panel
        .timers
        .push(GuiTimer::new(Duration::from_millis(100), 0));

    let positioning = Positioning::FollowHand {
        hand: LeftRight::Left,
        lerp: 1.0,
    };

    panel.update_layout(app)?;

    Ok(OverlayWindowConfig {
        name: WATCH_NAME.into(),
        z_order: Z_ORDER_WATCH,
        default_state: OverlayWindowState {
            grabbable: false,
            interactable: true,
            positioning,
            transform: Affine3A::from_scale_rotation_translation(
                Vec3::ONE * 0.115,
                WATCH_ROT,
                WATCH_POS,
            ),
            angle_fade: true,
            ..OverlayWindowState::default()
        },
        show_on_spawn: app.session.config.enable_watch,
        global: true,
        ..OverlayWindowConfig::from_backend(Box::new(panel))
    })
}

/// oneVR: where the watch sits for a tracked hand, in its wrist joint's space (XR_EXT_hand_tracking:
/// -Z toward the fingers, +Y out of the back of the hand, +X toward the left thumb and away from the
/// right one). It faces out of the back of the wrist turned toward the thumb, which faces the user
/// with the forearm across the chest, and its top points away from the user.
pub fn on_wrist(hand: LeftRight, scale: f32) -> Affine3A {
    // columns: the watch's right, up and front
    let (rotation, toward_thumb) = match hand {
        LeftRight::Left => (Mat3A::from_cols(Vec3A::NEG_Z, Vec3A::NEG_X, Vec3A::Y), -1.0),
        LeftRight::Right => (Mat3A::from_cols(Vec3A::Z, Vec3A::X, Vec3A::Y), 1.0),
    };
    // around the forearm, so it stays on its surface
    let tilt = Mat3A::from_rotation_z(toward_thumb * WRIST_WATCH_TILT);
    Affine3A {
        matrix3: tilt * rotation * scale,
        translation: tilt * WRIST_WATCH_POS,
    }
}

/// oneVR: where a toast shown on the watch sits on a controller: on the watch's face
pub fn toast_on_watch() -> (Vec3, Quat) {
    (WATCH_POS + WATCH_ROT * WATCH_TOAST_LIFT, WATCH_ROT)
}

/// oneVR: where a toast shown on the watch sits for a tracked hand, in its wrist joint's space
pub fn toast_on_wrist(hand: LeftRight, scale: f32) -> Affine3A {
    on_wrist(hand, 1.0)
        * Affine3A::from_translation(WATCH_TOAST_LIFT)
        * Affine3A::from_scale(Vec3::splat(scale))
}

fn sets_or_overlays(panel: &mut GuiPanel<WatchState>, app: &mut AppState) {
    let visible = if app.session.config.sets_on_watch {
        [false, true]
    } else {
        [true, false]
    };

    let widget = [
        panel
            .parser_state
            .get_widget_id("panels_root")
            .unwrap_or_default(),
        panel
            .parser_state
            .get_widget_id("sets_root")
            .unwrap_or_default(),
    ];

    for i in 0..2 {
        panel
            .layout
            .alterables
            .set_widget_visible(widget[i], visible[i]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A forearm across the chest with the back of the wrist up, the user above and behind it:
    /// the watch faces up and toward them, its top away from them, readable
    fn assert_readable(hand: LeftRight, fingers: Vec3A) {
        // the user's body is toward +Z, up is +Y
        let back = Vec3A::Y;
        let z = -fingers;
        let wrist = Affine3A {
            matrix3: Mat3A::from_cols(back.cross(z), back, z),
            translation: Vec3A::ZERO,
        };
        let watch = wrist * on_wrist(hand, 1.0);
        assert!(watch.matrix3.determinant() > 0.0, "{hand:?} is mirrored");
        let toward_user = Vec3A::new(0.0, 1.0, 1.0).normalize();
        assert!(
            watch.z_axis.abs_diff_eq(toward_user, 1e-6),
            "{hand:?} faces {}",
            watch.z_axis
        );
        let top = Vec3A::new(0.0, 1.0, -1.0).normalize();
        assert!(
            watch.y_axis.abs_diff_eq(top, 1e-6),
            "{hand:?} has its top at {}",
            watch.y_axis
        );
        // on the forearm, toward the user from its top
        assert!(watch.translation.z > 0.0 && watch.translation.y > 0.0);
    }

    #[test]
    fn the_left_wrists_watch_reads_upright() {
        assert_readable(LeftRight::Left, Vec3A::X);
    }

    #[test]
    fn the_right_wrists_watch_reads_upright() {
        assert_readable(LeftRight::Right, Vec3A::NEG_X);
    }
}
