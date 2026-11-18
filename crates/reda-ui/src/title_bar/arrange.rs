//! Where the Title bar's parts go across the window's width
//! (docs/spec/v0.1/design-language.md#title-bar): the quick strip and the window
//! controls never move, a drag gap is kept beside the controls, and as the window
//! narrows the command field shrinks to its minimum, then becomes an Icon button, then
//! the menus collapse into one menu button and the field comes back if that frees the
//! room.

use crate::widgets::{COMMAND_FIELD_MAX, COMMAND_FIELD_MIN};

/// The widths the arrangement is worked out from, in points, from the bar's leading
/// edge at zero.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Widths {
    /// The whole bar: the window's width.
    pub bar: f32,
    /// Where the menus may start: past any platform inset, the quick strip and the gap
    /// after it.
    pub menus_from: f32,
    /// The menu titles side by side.
    pub menus: f32,
    /// An Icon button: the collapsed menus' button, the command field as an icon, and
    /// the Preferences gear.
    pub button: f32,
    /// The window controls at the trailing end; zero where REDA draws none.
    pub controls: f32,
    /// The drag gap kept beside the window controls; zero where the bar does not drag.
    pub drag_gap: f32,
    /// The bar's own padding at the trailing end, and the gap between neighbours.
    pub gap: f32,
}

/// The command field's form.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Field {
    /// The field itself, `left` to `left + width`.
    Full { left: f32, width: f32 },
    /// An Icon button at `left`.
    Icon { left: f32 },
}

/// Where the parts go.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Arrangement {
    /// The menus are one menu button instead of a row of titles.
    pub menus_collapsed: bool,
    pub field: Field,
    /// The Preferences gear's leading edge.
    pub gear: f32,
}

/// Places the menus, the command field and the gear on a bar of `w`.
pub fn arrange(w: &Widths) -> Arrangement {
    let trailing = if w.controls > 0.0 || w.drag_gap > 0.0 {
        w.controls + w.drag_gap
    } else {
        w.gap
    };
    let gear = w.bar - trailing - w.button;
    // The menus and the field share what is left between the quick strip and the gear.
    let end = gear - w.gap;
    let room = end - w.menus_from;
    let full = |menus: f32| room - menus - w.gap >= COMMAND_FIELD_MIN;
    let (menus_collapsed, menus) = if full(w.menus) || room - w.menus - w.gap >= w.button {
        (false, w.menus)
    } else {
        (true, w.button)
    };
    let lo = w.menus_from + menus + w.gap;
    let field = if full(menus) {
        let width = (end - lo).min(COMMAND_FIELD_MAX);
        // Centred in the window where it fits, else pushed aside by its neighbours.
        let left = (w.bar / 2.0 - width / 2.0).clamp(lo, end - width);
        Field::Full { left, width }
    } else {
        Field::Icon { left: lo }
    };
    Arrangement {
        menus_collapsed,
        field,
        gear,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A bar like the main window's at Comfortable: 30 px buttons, a 6 px gap, a quick
    /// strip ending at 140, 330 px of menus, and REDA's window controls with the drag gap.
    fn bar(width: f32) -> Widths {
        Widths {
            bar: width,
            menus_from: 140.0,
            menus: 330.0,
            button: 30.0,
            controls: 3.0 * 46.0,
            drag_gap: 48.0,
            gap: 6.0,
        }
    }

    #[test]
    fn a_wide_window_centres_the_widest_field_in_the_window() {
        let a = arrange(&bar(1600.0));
        assert!(!a.menus_collapsed);
        assert_eq!(
            a.field,
            Field::Full {
                left: 590.0,
                width: 420.0
            }
        );
        // The gear, then the drag gap, then the controls.
        assert_eq!(a.gear, 1600.0 - 138.0 - 48.0 - 30.0);
    }

    #[test]
    fn a_field_that_cannot_be_centred_moves_aside_before_it_shrinks() {
        // Room for 420 after the menus, but not around the window's centre.
        let a = arrange(&bar(1200.0));
        let Field::Full { left, width } = a.field else {
            panic!("{a:?}")
        };
        assert_eq!(width, 420.0);
        assert_eq!(left, 140.0 + 330.0 + 6.0);
        assert!(left + width <= a.gear - 6.0);
    }

    #[test]
    fn the_field_shrinks_to_its_minimum_then_becomes_an_icon_button() {
        // Menus end at 476; the field ends 6 before the gear, at w - 222.
        let a = arrange(&bar(900.0));
        assert_eq!(
            a.field,
            Field::Full {
                left: 476.0,
                width: 202.0
            }
        );
        let a = arrange(&bar(878.0));
        assert_eq!(
            a.field,
            Field::Full {
                left: 476.0,
                width: 180.0
            }
        );
        let a = arrange(&bar(877.0));
        assert_eq!(a.field, Field::Icon { left: 476.0 });
        assert!(!a.menus_collapsed);
    }

    #[test]
    fn then_the_menus_collapse_and_the_field_comes_back() {
        // The icon still fits after the menus down to 728.
        let a = arrange(&bar(728.0));
        assert!(!a.menus_collapsed);
        assert_eq!(a.field, Field::Icon { left: 476.0 });
        // One px less: one menu button, and the field has room again.
        let a = arrange(&bar(727.0));
        assert!(a.menus_collapsed);
        assert_eq!(
            a.field,
            Field::Full {
                left: 176.0,
                width: 329.0
            }
        );
        // Narrower still, the field gives way to its icon once more.
        let a = arrange(&bar(578.0));
        assert!(a.menus_collapsed);
        assert_eq!(
            a.field,
            Field::Full {
                left: 176.0,
                width: 180.0
            }
        );
        assert_eq!(arrange(&bar(577.0)).field, Field::Icon { left: 176.0 });
    }

    #[test]
    fn the_drag_gap_and_controls_are_kept_whatever_the_width() {
        for width in [480.0, 700.0, 1000.0] {
            let a = arrange(&bar(width));
            assert_eq!(width - (a.gear + 30.0), 48.0 + 138.0, "at {width}");
        }
    }

    #[test]
    fn without_controls_or_dragging_the_gear_keeps_only_the_bar_padding() {
        let w = Widths {
            controls: 0.0,
            drag_gap: 0.0,
            ..bar(1000.0)
        };
        assert_eq!(arrange(&w).gear, 1000.0 - 6.0 - 30.0);
    }
}
