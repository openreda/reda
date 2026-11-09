//! Ports and Power Ports.
//!
//! Both are drawn in their own frame, then turned by their rotation about their connection
//! point. A Port is a flag reaching along +x from its point (or -x when its Wire leaves
//! along +x, so the flag reaches away from the Wire), with its name inside and its
//! cross-reference note past its far end. The flag is pointed
//! at the far end for `out`, at the connection end for `in`, at both for `bidirectional`.
//! A Power Port is a stem with a fixed symbol at its end and its name beyond; ground
//! styles hang down (-y), the rest stand up (+y).

use reda_model::geom::{Millideg, Nm, Point};
use reda_model::{Port, PortDirection, PowerPortStyle, SchematicSheet, Uuid};

use super::{Builder, NoteLink, SchematicStyle, plain_name};
use crate::color::SchematicColor as C;
use crate::scene::{Align, HAlign, Link, Primitive, VAlign};
use crate::text::{TextStyle, advance, span};
use crate::transform::rotate;

const OUTLINE: i64 = 152_400;
const NAME_TEXT: Nm = Nm::from_um(1_270);
/// Half the height of a Port's flag.
const FLAG_HALF: i64 = 1_016_000;
/// How far a Port's pointed end reaches past its straight sides.
const FLAG_TIP: i64 = FLAG_HALF;
/// Room between a Port's name and its outline.
const FLAG_PAD: i64 = 508_000;
/// Room between a Port's flag and its cross-reference note.
const NOTE_GAP: i64 = 508_000;
/// A Power Port's stem.
const STEM: i64 = 1_270_000;
/// Half the width of a Power Port's symbol.
const SYMBOL_HALF: i64 = 1_270_000;
/// Room between a Power Port's symbol and its name.
const NAME_GAP: i64 = 508_000;
/// Segments of a wave-style Power Port's one period.
const WAVE_STEPS: i64 = 12;

/// One object's drawing in its own frame, placed on the sheet as it goes.
struct Local<'a> {
    b: &'a mut Builder,
    object: Uuid,
    color: C,
    at: Point,
    rotation: Millideg,
}

impl Local<'_> {
    fn place(&self, (x, y): (i64, i64)) -> Point {
        let (x, y) = rotate(x, y, self.rotation);
        Point::new(self.at.x + Nm(x), self.at.y + Nm(y))
    }

    fn line(&mut self, a: (i64, i64), z: (i64, i64)) {
        let (a, z) = (self.place(a), self.place(z));
        self.b.push(
            self.object,
            self.color,
            Primitive::Capsule {
                a,
                b: z,
                width: Nm(OUTLINE),
            },
        );
    }

    fn closed(&mut self, points: &[(i64, i64)]) {
        for (i, &a) in points.iter().enumerate() {
            self.line(a, points[(i + 1) % points.len()]);
        }
    }

    fn ring(&mut self, center: (i64, i64), radius: i64) {
        let center = self.place(center);
        self.b.push(
            self.object,
            self.color,
            Primitive::Ring {
                center,
                outer: Nm(radius + OUTLINE / 2),
                inner: Nm(radius - OUTLINE / 2),
            },
        );
    }

    /// The name, centred on `center` and running along the object's rotation (turned to
    /// read left to right when that would read upside down).
    fn name(&mut self, name: &str, single_negation: bool, center: (i64, i64)) {
        let at = self.place(center);
        self.b.name(
            self.object,
            self.color,
            name,
            single_negation,
            at,
            TextStyle {
                height: NAME_TEXT,
                rotation: self.rotation,
                align: Align::new(HAlign::Center, VAlign::Middle),
                view_label: false,
            },
        );
    }
}

impl Local<'_> {
    /// A Port's cross-reference note, starting at `start` and reading away from the
    /// flag.
    /// Each of `links` becomes a link over its entry's text.
    fn note(&mut self, note: &str, links: &[NoteLink], start: (i64, i64)) {
        let at = self.place(start);
        let style = TextStyle {
            height: NAME_TEXT,
            rotation: self.rotation,
            align: Align::new(HAlign::Left, VAlign::Middle),
            view_label: false,
        };
        self.b.text(self.object, self.color, note, at, style);
        let mut from = 0;
        for link in links {
            let Some(i) = note[from..].find(&link.entry).map(|i| i + from) else {
                continue;
            };
            from = i + link.entry.len();
            self.b.scene.links.push(Link {
                object: self.object,
                area: span(note, i..from, at, style),
                sheet: link.sheet,
                at: link.at,
            });
        }
    }
}

pub(super) fn ports(b: &mut Builder, sheet: &SchematicSheet, style: &SchematicStyle) {
    let single_negation = style.single_negation;
    let (h, tip, pad) = (FLAG_HALF, FLAG_TIP, FLAG_PAD);
    for port in sheet.ports() {
        let shown = plain_name(&port.name, single_negation);
        let len = tip + pad + advance(&shown, NAME_TEXT).0 + pad + tip;
        let outline: &[(i64, i64)] = match port.direction {
            PortDirection::Out => &[(0, h), (len - tip, h), (len, 0), (len - tip, -h), (0, -h)],
            PortDirection::In => &[(0, 0), (tip, h), (len, h), (len, -h), (tip, -h)],
            PortDirection::Bidirectional => &[
                (0, 0),
                (tip, h),
                (len - tip, h),
                (len, 0),
                (len - tip, -h),
                (tip, -h),
            ],
        };
        // The flag reaches away from its Wire: one that leaves on the flag's side turns it
        // half a turn, which keeps an `out` point at the far end and an `in` point at the
        // Wire.
        let rotation = match wire_on_flag_side(sheet, port) {
            true => (port.rotation + Millideg::from_deg(180)).normalized(),
            false => port.rotation,
        };
        let mut l = Local {
            b,
            object: port.uuid,
            color: C::Port,
            at: port.at,
            rotation,
        };
        l.closed(outline);
        l.name(&port.name, single_negation, (len / 2, 0));
        if let Some(note) = style.cross_references.get(&port.uuid) {
            let links = style.cross_reference_links.get(&port.uuid);
            l.note(note, links.map_or(&[], Vec::as_slice), (len + NOTE_GAP, 0));
        }
    }
}

/// Whether a Wire segment ending on `port`'s point leaves it towards the side the flag
/// reaches.
fn wire_on_flag_side(sheet: &SchematicSheet, port: &Port) -> bool {
    let (fx, fy) = rotate(1_000_000, 0, port.rotation);
    sheet.wires().flat_map(|w| w.segments()).any(|(a, b)| {
        let other = match (a == port.at, b == port.at) {
            (true, _) => b,
            (_, true) => a,
            _ => return false,
        };
        let (vx, vy) = ((other.x - port.at.x).0, (other.y - port.at.y).0);
        i128::from(vx) * i128::from(fx) + i128::from(vy) * i128::from(fy) > 0
    })
}

pub(super) fn power_ports(b: &mut Builder, sheet: &SchematicSheet, single_negation: bool) {
    let (s, w) = (STEM, SYMBOL_HALF);
    for port in sheet.power_ports() {
        let mut l = Local {
            b,
            object: port.uuid,
            color: C::PowerPort,
            at: port.at,
            rotation: port.rotation,
        };
        // How far the symbol reaches from the connection point, signed: + up, - down.
        let reach = match port.style {
            PowerPortStyle::Bar => {
                l.line((0, 0), (0, s));
                l.line((-w, s), (w, s));
                s
            }
            PowerPortStyle::Arrow => {
                l.line((0, 0), (0, s));
                l.closed(&[(-w / 2, s), (w / 2, s), (0, s + w)]);
                s + w
            }
            PowerPortStyle::Circle => {
                l.line((0, 0), (0, s));
                l.ring((0, s + w / 2), w / 2);
                s + w
            }
            PowerPortStyle::Wave => {
                l.line((0, 0), (0, s));
                let amp = w / 4;
                let wave = |i: i64| {
                    let x = -w + 2 * w * i / WAVE_STEPS;
                    let phase = std::f64::consts::TAU * i as f64 / WAVE_STEPS as f64;
                    (x, s + (amp as f64 * phase.sin()).round() as i64)
                };
                for i in 0..WAVE_STEPS {
                    l.line(wave(i), wave(i + 1));
                }
                s + amp
            }
            PowerPortStyle::Ground => {
                let gap = w / 3;
                l.line((0, 0), (0, -s));
                for (k, half) in [w, w * 2 / 3, w / 3].into_iter().enumerate() {
                    let y = -s - gap * k as i64;
                    l.line((-half, y), (half, y));
                }
                -(s + 2 * gap)
            }
            PowerPortStyle::SignalGround => {
                l.line((0, 0), (0, -s));
                l.closed(&[(-w, -s), (w, -s), (0, -s - w)]);
                -(s + w)
            }
            PowerPortStyle::Earth => {
                let gap = w / 2;
                l.line((0, 0), (0, -s));
                l.line((-w, -s), (w, -s));
                for x in [-w, 0, w] {
                    l.line((x, -s), (x - gap, -s - gap));
                }
                -(s + gap)
            }
        };
        if !port.hide_name {
            let beyond = NAME_GAP + NAME_TEXT.0 / 2;
            l.name(
                &port.name,
                single_negation,
                (0, reach + reach.signum() * beyond),
            );
        }
    }
}
