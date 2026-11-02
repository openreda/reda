//! Typed reads from S-expression nodes, failing with the node's place in the file.

use reda_model::geom::{Millideg, Nm, Point};

use crate::number;
use crate::sexpr::{Sexp, SexpError};

pub type Read<T> = Result<T, SexpError>;

pub fn fail<T>(node: &Sexp, message: impl Into<String>) -> Read<T> {
    Err(SexpError {
        pos: node.pos(),
        message: message.into(),
    })
}

/// The child list `name`, which must be there.
pub fn need<'a>(node: &'a Sexp, name: &str) -> Read<&'a Sexp> {
    match node.child(name) {
        Some(c) => Ok(c),
        None => fail(
            node,
            format!("`({} …)` has no `({name} …)`", node.head().unwrap_or("")),
        ),
    }
}

/// Argument `n` as text, which must be there.
pub fn text(node: &Sexp, n: usize) -> Read<&str> {
    match node.arg(n) {
        Some(t) => Ok(t),
        None => fail(
            node,
            format!(
                "`({} …)` is missing argument {}",
                node.head().unwrap_or(""),
                n + 1
            ),
        ),
    }
}

/// Argument `n` as millimetres.
pub fn mm(node: &Sexp, n: usize) -> Read<Nm> {
    let t = text(node, n)?;
    match number::mm(t) {
        Some(v) => Ok(v),
        None => fail(&node.args()[n], format!("{t:?} is not a number")),
    }
}

/// Argument `n` as degrees; zero when it is absent.
pub fn degrees_or_zero(node: &Sexp, n: usize) -> Read<Millideg> {
    match node.arg(n) {
        None => Ok(Millideg(0)),
        Some(t) => match number::degrees(t) {
            Some(v) => Ok(v),
            None => fail(&node.args()[n], format!("{t:?} is not an angle")),
        },
    }
}

/// The first two arguments as a point, as in `(at 1 2)` or `(xy 1 2)`.
pub fn point(node: &Sexp) -> Read<Point> {
    Ok(Point::new(mm(node, 0)?, mm(node, 1)?))
}

/// The point of child `name`, as in `(start 1 2)`.
pub fn point_of(node: &Sexp, name: &str) -> Read<Point> {
    point(need(node, name)?)
}

/// The points of `(pts (xy …) …)`.
pub fn points(node: &Sexp) -> Read<Vec<Point>> {
    need(node, "pts")?.children("xy").map(point).collect()
}

/// A stroke width: `(stroke (width w))` or the older `(width w)`; `None` when absent.
pub fn width(node: &Sexp) -> Read<Option<Nm>> {
    let w = node
        .child("stroke")
        .and_then(|s| s.child("width"))
        .or_else(|| node.child("width"));
    w.map(|w| mm(w, 0)).transpose()
}

/// The text height of `(effects (font (size h w)))`, when given.
pub fn font_size(node: &Sexp) -> Read<Option<Nm>> {
    let size = node
        .child("effects")
        .and_then(|e| e.child("font"))
        .and_then(|f| f.child("size"));
    size.map(|s| mm(s, 0)).transpose()
}

/// Whether the text is hidden: `hide` in the node or in its `effects`.
pub fn hidden(node: &Sexp) -> bool {
    node.flag("hide") || node.child("effects").is_some_and(|e| e.flag("hide"))
}
