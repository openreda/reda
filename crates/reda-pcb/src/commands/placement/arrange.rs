//! `pcb.align`, `pcb.distribute` and `pcb.arrange_in_rect`.

use std::cmp::Ordering;

use reda_command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Length, Meta, Registry, Unavailable,
};
use reda_model::geom::{Nm, Point};
use reda_model::{PcbDocument, Uuid};
use schemars::JsonSchema;
use serde::Deserialize;

use super::{Item, Op, Placed, place, refuse_locked, resolve};
use crate::commands::outline::PointParam;
use crate::commands::{needs_pcb, non_negative, pcb};
use crate::placed::Bounds;

pub(super) fn register_all(registry: &mut Registry) {
    registry.add(Align);
    registry.add(Distribute);
    registry.add(ArrangeInRect);
}

/// The space between Footprints that Arrange within rectangle leaves by default.
const DEFAULT_SPACING: Nm = Nm::from_mm(1);

/// What [`Align`] lines up. Up is +y.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AlignTo {
    /// Left edges on the leftmost.
    Left,
    /// Right edges on the rightmost.
    Right,
    /// Top edges on the topmost.
    Top,
    /// Bottom edges on the bottommost.
    Bottom,
    /// Centres on one vertical line through the middle of the selection's extent.
    CenterX,
    /// Centres on one horizontal line through the middle of the selection's extent.
    CenterY,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AlignParams {
    /// The Footprints, Free Pads and Placement Origin to align, by UUID. A Footprint's
    /// extent is its Pads and drawing.
    pub objects: Vec<Uuid>,
    pub to: AlignTo,
}

pub struct Align;

impl Command for Align {
    const META: Meta = Meta {
        id: "pcb.align",
        title: "Align",
        description: "Aligns the selected Footprints' (and Free Pads') left, right, top or \
                      bottom edges, or their centres, moving each as a whole. Up is +y. \
                      Refused when any of them is locked.",
        category: "pcb",
        icon: "align",
        kind: Kind::Edit,
    };
    type Params = AlignParams;
    type Output = Placed;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: AlignParams) -> Result<Placed, CommandError> {
        let to = p.to;
        arrange(cx, &p.objects, |boxes| {
            let all = boxes
                .iter()
                .copied()
                .reduce(Bounds::union)
                .expect("a selection is never empty");
            let zero = Nm::ZERO;
            let offsets = boxes.iter().map(|each| {
                let (dx, dy) = match to {
                    AlignTo::Left => (all.min.x - each.min.x, zero),
                    AlignTo::Right => (all.max.x - each.max.x, zero),
                    AlignTo::Top => (zero, all.max.y - each.max.y),
                    AlignTo::Bottom => (zero, all.min.y - each.min.y),
                    AlignTo::CenterX => (all.center().x - each.center().x, zero),
                    AlignTo::CenterY => (zero, all.center().y - each.center().y),
                };
                Point::new(dx, dy)
            });
            Ok(offsets.collect())
        })
    }
}

/// Which way [`Distribute`] spaces objects.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Horizontal,
    Vertical,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DistributeParams {
    /// The Footprints, Free Pads and Placement Origin to space out, by UUID.
    pub objects: Vec<Uuid>,
    pub direction: Direction,
}

pub struct Distribute;

impl Command for Distribute {
    const META: Meta = Meta {
        id: "pcb.distribute",
        title: "Distribute",
        description: "Spaces the selected Footprints' centres evenly, horizontally or \
                      vertically, between the two outermost, which stay put. Refused when \
                      any of them is locked.",
        category: "pcb",
        icon: "distribute",
        kind: Kind::Edit,
    };
    type Params = DistributeParams;
    type Output = Placed;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: DistributeParams) -> Result<Placed, CommandError> {
        let horizontal = p.direction == Direction::Horizontal;
        let along = move |b: &Bounds| {
            let c = b.center();
            if horizontal { c.x.0 } else { c.y.0 }
        };
        arrange(cx, &p.objects, |boxes| {
            let n = boxes.len();
            let mut out = vec![Point::default(); n];
            if n < 3 {
                return Ok(out);
            }
            let mut order: Vec<usize> = (0..n).collect();
            order.sort_by_key(|&i| along(&boxes[i]));
            let first = i128::from(along(&boxes[order[0]]));
            let last = i128::from(along(&boxes[order[n - 1]]));
            for (k, &i) in order.iter().enumerate() {
                let want = first + ((last - first) * k as i128).div_euclid((n - 1) as i128);
                let d = Nm((want - i128::from(along(&boxes[i]))) as i64);
                out[i] = match horizontal {
                    true => Point::new(d, Nm::ZERO),
                    false => Point::new(Nm::ZERO, d),
                };
            }
            Ok(out)
        })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArrangeInRectParams {
    /// The Footprints and Free Pads to arrange, by UUID.
    pub objects: Vec<Uuid>,
    /// One corner of the rectangle.
    pub from: PointParam,
    /// The opposite corner.
    pub to: PointParam,
    /// The space left between neighbours; 1 mm when left out.
    #[serde(default)]
    pub spacing: Option<Length>,
}

pub struct ArrangeInRect;

impl Command for ArrangeInRect {
    const META: Meta = Meta {
        id: "pcb.arrange_in_rect",
        title: "Arrange Within Rectangle",
        description: "Packs the selected Footprints into a rectangle in rows, from its top-left \
                      corner, left to right and then down, in Designator order, `spacing` \
                      (1 mm) apart. Their rotation and side are kept. Refused when they do not \
                      fit or any of them is locked.",
        category: "pcb",
        icon: "arrange",
        kind: Kind::Edit,
    };
    type Params = ArrangeInRectParams;
    type Output = Placed;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: ArrangeInRectParams) -> Result<Placed, CommandError> {
        let spacing = p
            .spacing
            .map_or(Ok(DEFAULT_SPACING), |s| non_negative("/spacing", s))?;
        let rect = Bounds::of_points([p.from.point(), p.to.point()]).expect("two corners");
        let pcb = pcb(cx)?;
        let items = resolve(pcb, &p.objects)?;
        let mut order: Vec<usize> = (0..items.len()).collect();
        order.sort_by(|&a, &b| designator_order(&items[a], &items[b]));
        arrange(cx, &p.objects, |boxes| {
            let mut out = vec![Point::default(); boxes.len()];
            let (mut x, mut top) = (rect.min.x, rect.max.y);
            let mut row_height = Nm::ZERO;
            for &i in &order {
                let b = boxes[i];
                if x > rect.min.x && x + b.width() > rect.max.x {
                    x = rect.min.x;
                    top = top - row_height - spacing;
                    row_height = Nm::ZERO;
                }
                if x + b.width() > rect.max.x || top - b.height() < rect.min.y {
                    return Err(CommandError::rejected(
                        "the selection does not fit in the rectangle",
                    ));
                }
                out[i] = Point::new(x - b.min.x, top - b.max.y);
                x = x + b.width() + spacing;
                row_height = row_height.max(b.height());
            }
            Ok(out)
        })
    }
}

/// Footprints by Designator in natural order (R2 before R10), then Free Pads and the
/// Placement Origin by UUID.
fn designator_order(a: &Item, b: &Item) -> Ordering {
    match (a, b) {
        (Item::Footprint(a), Item::Footprint(b)) => {
            natural_cmp(&a.designator, &b.designator).then(a.uuid.cmp(&b.uuid))
        }
        (Item::Footprint(_), _) => Ordering::Less,
        (_, Item::Footprint(_)) => Ordering::Greater,
        _ => a.uuid().cmp(&b.uuid()),
    }
}

/// Compares text with runs of digits as numbers, so R2 comes before R10.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a, b);
    loop {
        match (a.chars().next(), b.chars().next()) {
            (None, None) => return Ordering::Equal,
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let da = a.len() - a.trim_start_matches(|c: char| c.is_ascii_digit()).len();
                let db = b.len() - b.trim_start_matches(|c: char| c.is_ascii_digit()).len();
                let (na, nb) = (
                    a[..da].trim_start_matches('0'),
                    b[..db].trim_start_matches('0'),
                );
                let ord = na.len().cmp(&nb.len()).then(na.cmp(nb));
                if ord != Ordering::Equal {
                    return ord;
                }
                (a, b) = (&a[da..], &b[db..]);
            }
            (Some(x), Some(y)) => {
                if x != y {
                    return x.cmp(&y);
                }
                (a, b) = (&a[x.len_utf8()..], &b[y.len_utf8()..]);
            }
        }
    }
}

/// Moves each object by the offset `offsets` gives it from every object's box, in order.
fn arrange(
    cx: &mut Context<'_>,
    uuids: &[Uuid],
    offsets: impl FnOnce(&[Bounds]) -> Result<Vec<Point>, CommandError>,
) -> Result<Placed, CommandError> {
    let pcb: &PcbDocument = pcb(cx)?;
    let items = resolve(pcb, uuids)?;
    refuse_locked(&items)?;
    let boxes: Vec<Bounds> = items.iter().map(|i| i.bounds(pcb)).collect();
    let ops: Vec<Op> = offsets(&boxes)?.into_iter().map(Op::Translate).collect();
    place(cx, items, ops)
}
