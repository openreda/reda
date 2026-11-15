//! The Glyph set: the small shapes that are part of a control's own drawing
//! (docs/spec/v0.1/design-language.md#icons-and-glyphs).
//!
//! A Glyph is one colour and takes the colour of the text beside it, so it follows hover,
//! muted, disabled and white-on-accent states. Each is drawn as 1.5 px round-capped
//! strokes in a 16 px box that only UI scale changes, snapped to the pixel grid. An Icon,
//! which stands for a Command and carries role colours, is not one of these.

use egui::{Color32, Painter, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, Vec2, Widget, vec2};

use egui::emath::GuiRounding as _;

use crate::theme::TokensExt as _;

/// The side of the box every Glyph is drawn in.
pub const BOX: f32 = 16.0;
/// The width of a Glyph's strokes.
pub const STROKE: f32 = 1.5;

/// The named set.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Glyph {
    /// A combo box, an open section.
    ChevronDown,
    ChevronUp,
    /// A submenu, a closed section.
    ChevronRight,
    ChevronLeft,
    /// A checked box, a checked menu item, a success.
    Tick,
    /// An invalid field, an Error.
    Cross,
    /// A mixed check box, an entry that is off or waived.
    Minus,
    /// The close × of a tab, a dialog and the window.
    Close,
    /// From one thing to another: a change, a link that leaves.
    Arrow,
    /// A modified Document's dot.
    Dot,
    /// The window controls; the close button is [`Glyph::Close`].
    Minimise,
    Maximise,
    Restore,
    /// The search glass at the leading end of a search field.
    Search,
    /// The lock toggle in list headers and rows.
    Lock,
    /// The filter toggle in list headers.
    Funnel,
    /// A Warning. It and [`Glyph::Cross`] take the semantic colours.
    Warning,
    /// A table column sorted ascending, and descending.
    SortUp,
    SortDown,
    /// The snap palette: a grid of dots with a ring on the one snapped to.
    Snap,
}

impl Glyph {
    /// Every Glyph with its name, in the order the gallery shows them.
    pub const ALL: [(Glyph, &'static str); 20] = [
        (Glyph::ChevronDown, "chevron down"),
        (Glyph::ChevronUp, "chevron up"),
        (Glyph::ChevronRight, "chevron right"),
        (Glyph::ChevronLeft, "chevron left"),
        (Glyph::Tick, "tick"),
        (Glyph::Cross, "cross"),
        (Glyph::Minus, "minus"),
        (Glyph::Close, "close"),
        (Glyph::Arrow, "arrow"),
        (Glyph::Dot, "modified dot"),
        (Glyph::Minimise, "minimise"),
        (Glyph::Maximise, "maximise"),
        (Glyph::Restore, "restore"),
        (Glyph::Search, "search"),
        (Glyph::Lock, "lock"),
        (Glyph::Funnel, "funnel"),
        (Glyph::Warning, "warning"),
        (Glyph::SortUp, "sort ascending"),
        (Glyph::SortDown, "sort descending"),
        (Glyph::Snap, "snap"),
    ];

    /// Paints the Glyph with its box centred on `center`, in `color`.
    pub fn paint(self, painter: &Painter, center: Pos2, color: Color32) {
        // The box lands on whole pixels, so the half-pixel coordinates below stay crisp.
        let origin = (center - Vec2::splat(BOX / 2.0)).round_to_pixels(painter.pixels_per_point());
        let pen = Stroke::new(STROKE, color);
        let at = |(x, y): (f32, f32)| origin + vec2(x, y);
        // An open path with round caps: a dot of the stroke's width on each end.
        let open = |points: &[(f32, f32)]| {
            let points: Vec<Pos2> = points.iter().copied().map(at).collect();
            for end in [points[0], points[points.len() - 1]] {
                painter.circle_filled(end, STROKE / 2.0, color);
            }
            painter.add(Shape::line(points, pen));
        };
        let closed = |points: &[(f32, f32)]| {
            let points: Vec<Pos2> = points.iter().copied().map(at).collect();
            painter.add(Shape::closed_line(points, pen));
        };
        match self {
            Glyph::ChevronDown => open(&[(4.5, 6.5), (8.0, 10.0), (11.5, 6.5)]),
            Glyph::ChevronUp => open(&[(4.5, 9.5), (8.0, 6.0), (11.5, 9.5)]),
            Glyph::ChevronRight => open(&[(6.5, 4.5), (10.0, 8.0), (6.5, 11.5)]),
            Glyph::ChevronLeft => open(&[(9.5, 4.5), (6.0, 8.0), (9.5, 11.5)]),
            Glyph::Tick => open(&[(4.0, 8.5), (6.75, 11.25), (12.0, 5.0)]),
            Glyph::Cross => {
                open(&[(5.0, 5.0), (11.0, 11.0)]);
                open(&[(5.0, 11.0), (11.0, 5.0)]);
            }
            Glyph::Minus => open(&[(4.5, 8.0), (11.5, 8.0)]),
            Glyph::Close => {
                open(&[(4.0, 4.0), (12.0, 12.0)]);
                open(&[(4.0, 12.0), (12.0, 4.0)]);
            }
            Glyph::Arrow => {
                open(&[(3.0, 8.0), (13.0, 8.0)]);
                open(&[(9.5, 4.5), (13.0, 8.0), (9.5, 11.5)]);
            }
            Glyph::Dot => {
                painter.circle_filled(at((8.0, 8.0)), DOT_RADIUS, color);
            }
            Glyph::Minimise => open(&[(3.5, 8.5), (12.5, 8.5)]),
            Glyph::Maximise => {
                let rect = Rect::from_min_max(at((3.5, 3.5)), at((12.5, 12.5)));
                painter.rect_stroke(rect, WINDOW_CORNER, pen, egui::StrokeKind::Middle);
            }
            Glyph::Restore => {
                let rect = Rect::from_min_max(at((3.5, 5.5)), at((10.5, 12.5)));
                painter.rect_stroke(rect, WINDOW_CORNER, pen, egui::StrokeKind::Middle);
                open(&[(6.0, 3.5), (12.5, 3.5), (12.5, 10.0)]);
            }
            Glyph::Search => {
                painter.circle_stroke(at((7.0, 7.0)), 3.75, pen);
                open(&[(9.75, 9.75), (12.75, 12.75)]);
            }
            Glyph::Lock => {
                let body = Rect::from_min_max(at((4.5, 7.5)), at((11.5, 12.5)));
                painter.rect_stroke(body, WINDOW_CORNER, pen, egui::StrokeKind::Middle);
                // The shackle: two legs joined by a half circle.
                let mut shackle = vec![(5.75, 7.5)];
                for step in 0..=8 {
                    let a = std::f32::consts::PI * (1.0 - step as f32 / 8.0);
                    shackle.push((8.0 + 2.25 * a.cos(), 5.75 - 2.25 * a.sin()));
                }
                shackle.push((10.25, 7.5));
                open(&shackle);
            }
            Glyph::Funnel => closed(&[
                (3.5, 4.0),
                (12.5, 4.0),
                (9.25, 8.25),
                (9.25, 12.5),
                (6.75, 11.25),
                (6.75, 8.25),
            ]),
            Glyph::Warning => {
                closed(&[(8.0, 3.0), (13.5, 12.5), (2.5, 12.5)]);
                open(&[(8.0, 6.75), (8.0, 9.0)]);
                painter.circle_filled(at((8.0, 10.75)), STROKE / 2.0, color);
            }
            Glyph::SortUp => {
                open(&[(8.0, 12.5), (8.0, 3.5)]);
                open(&[(4.5, 7.0), (8.0, 3.5), (11.5, 7.0)]);
            }
            Glyph::SortDown => {
                open(&[(8.0, 3.5), (8.0, 12.5)]);
                open(&[(4.5, 9.0), (8.0, 12.5), (11.5, 9.0)]);
            }
            Glyph::Snap => {
                for x in [2.5, 8.0, 13.5] {
                    for y in [2.5, 8.0, 13.5] {
                        painter.circle_filled(at((x, y)), STROKE / 2.0, color);
                    }
                }
                painter.circle_stroke(at((8.0, 8.0)), SNAP_RING, pen);
            }
        }
    }
}

/// The ring on the snap Glyph's middle dot.
const SNAP_RING: f32 = 3.5;
/// The modified dot is a filled disc, not a stroke.
const DOT_RADIUS: f32 = 3.0;
/// The corner of the boxes inside the window-control and lock Glyphs.
const WINDOW_CORNER: f32 = 1.5;

/// A Glyph as a widget: its 16 px box, in the primary text colour unless told otherwise.
pub fn glyph(glyph: Glyph) -> GlyphMark {
    GlyphMark { glyph, color: None }
}

#[must_use = "add it with ui.add"]
pub struct GlyphMark {
    glyph: Glyph,
    color: Option<Color32>,
}

impl GlyphMark {
    /// The colour of the text this Glyph sits beside.
    pub fn color(mut self, color: Color32) -> Self {
        self.color = Some(color);
        self
    }
}

impl Widget for GlyphMark {
    fn ui(self, ui: &mut Ui) -> Response {
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(BOX), Sense::hover());
        if ui.is_rect_visible(rect) {
            let color = self.color.unwrap_or(ui.tokens().color.text.primary);
            self.glyph.paint(ui.painter(), rect.center(), color);
        }
        response
    }
}
