//! The router over hand-made obstacle layouts. Points are in mil on a 50 mil grid.

use reda_model::geom::{Bounds, Nm, Point};

use super::{Avoid, Obstacles, Options, RouteError, route};

fn p(x: i64, y: i64) -> Point {
    Point::new(Nm::from_mil(x), Nm::from_mil(y))
}

fn mil(points: &[Point]) -> Vec<(i64, i64)> {
    points
        .iter()
        .map(|q| (q.x.0 / 25_400, q.y.0 / 25_400))
        .collect()
}

fn options() -> Options {
    Options {
        grid: Nm::from_mil(50),
        avoid: Avoid::Medium,
        timeout_ms: 3000,
    }
}

fn run(obstacles: &Obstacles, from: (i64, i64), to: (i64, i64)) -> Vec<(i64, i64)> {
    let route = route(obstacles, p(from.0, from.1), p(to.0, to.1), &options()).unwrap();
    mil(&route.points)
}

#[test]
fn an_open_sheet_routes_straight() {
    assert_eq!(
        run(&Obstacles::default(), (0, 0), (500, 0)),
        [(0, 0), (500, 0)]
    );
}

#[test]
fn an_open_sheet_routes_with_one_corner() {
    let points = run(&Obstacles::default(), (0, 0), (500, 300));
    assert_eq!(points.len(), 3, "{points:?}");
    assert_eq!(points[0], (0, 0));
    assert_eq!(points[2], (500, 300));
}

fn body(x0: i64, y0: i64, x1: i64, y1: i64) -> Bounds {
    Bounds {
        min: p(x0, y0),
        max: p(x1, y1),
    }
}

/// Every unit step of a route, as the grid points it visits in order.
fn visited(points: &[(i64, i64)]) -> Vec<(i64, i64)> {
    let mut out = vec![points[0]];
    for pair in points.windows(2) {
        let ((x0, y0), (x1, y1)) = (pair[0], pair[1]);
        assert!(x0 == x1 || y0 == y1, "a 90° route: {points:?}");
        let n = ((x1 - x0).abs() + (y1 - y0).abs()) / 50;
        for k in 1..=n {
            out.push((x0 + (x1 - x0) * k / n, y0 + (y1 - y0) * k / n));
        }
    }
    out
}

fn inside(q: (i64, i64), b: (i64, i64, i64, i64)) -> bool {
    b.0 <= q.0 && q.0 <= b.2 && b.1 <= q.1 && q.1 <= b.3
}

#[test]
fn a_body_in_the_way_is_gone_round() {
    let obstacles = Obstacles {
        boxes: vec![body(200, -200, 300, 200)],
        ..Obstacles::default()
    };
    let points = run(&obstacles, (0, 0), (500, 0));
    assert_eq!(points.first(), Some(&(0, 0)));
    assert_eq!(points.last(), Some(&(500, 0)));
    assert!(
        visited(&points)
            .iter()
            .all(|&q| !inside(q, (200, -200, 300, 200))),
        "{points:?}"
    );
    // A grid step clear of it, where nothing else is in the way.
    assert!(
        visited(&points)
            .iter()
            .all(|&q| !inside(q, (150, -250, 350, 250))),
        "{points:?}"
    );
}

#[test]
fn a_route_never_runs_along_a_body_edge() {
    // The straight line runs along the top edge of the body.
    let obstacles = Obstacles {
        boxes: vec![body(200, 0, 300, 200)],
        ..Obstacles::default()
    };
    let points = run(&obstacles, (0, 0), (500, 0));
    assert!(
        visited(&points)
            .iter()
            .all(|&q| !inside(q, (200, 0, 300, 200))),
        "{points:?}"
    );
}

#[test]
fn a_route_leaves_a_pin_outward_and_reaches_another() {
    // Two Units side by side, Pins facing each other; the route joins them directly.
    let obstacles = Obstacles {
        boxes: vec![body(-200, -100, 0, 100), body(500, -100, 700, 100)],
        points: vec![p(0, 0), p(0, 50), p(500, 0), p(500, -50)],
        ..Obstacles::default()
    };
    assert_eq!(run(&obstacles, (0, 0), (500, 0)), [(0, 0), (500, 0)]);
}

#[test]
fn a_route_never_passes_over_a_pin_or_label() {
    for point in [(250, 0), (260, 0)] {
        let obstacles = Obstacles {
            points: vec![p(point.0, point.1)],
            ..Obstacles::default()
        };
        let points = run(&obstacles, (0, 0), (500, 0));
        assert!(points.len() > 2, "past {point:?}: {points:?}");
        assert!(
            visited(&points).iter().all(|&q| q != (250, 0)),
            "{points:?}"
        );
    }
}

#[test]
fn a_route_crosses_a_wire_rather_than_detour_far() {
    let obstacles = Obstacles {
        wires: vec![vec![p(250, -2000), p(250, 2000)]],
        ..Obstacles::default()
    };
    let route = route(&obstacles, p(0, 0), p(500, 0), &options()).unwrap();
    assert_eq!(mil(&route.points), [(0, 0), (500, 0)]);
    assert_eq!(route.crossings, 1);
}

#[test]
fn avoid_cutting_wires_trades_length_for_crossings() {
    // A short Wire across the straight path: going round it costs six grid steps more.
    let obstacles = Obstacles {
        wires: vec![vec![p(250, -100), p(250, 100)]],
        ..Obstacles::default()
    };
    let at = |avoid| {
        let options = Options { avoid, ..options() };
        route(&obstacles, p(0, 0), p(500, 0), &options).unwrap()
    };
    assert_eq!(at(Avoid::Low).crossings, 1);
    let high = at(Avoid::High);
    assert_eq!(high.crossings, 0);
    assert!(
        visited(&mil(&high.points))
            .iter()
            .all(|q| q.0 != 250 || q.1.abs() > 100),
        "{:?}",
        high.points
    );
}

#[test]
fn a_route_never_runs_along_or_turns_on_a_wire() {
    // A Wire lies on the straight path; the route must step off it.
    let obstacles = Obstacles {
        wires: vec![vec![p(100, 0), p(400, 0)]],
        ..Obstacles::default()
    };
    let points = run(&obstacles, (0, 0), (500, 0));
    for q in visited(&points) {
        assert!(!(q.1 == 0 && (100..=400).contains(&q.0)), "{points:?}");
    }
    // Turning on a Wire: a long horizontal Wire between the ends, which are offset in x.
    let obstacles = Obstacles {
        wires: vec![vec![p(-2000, 200), p(2000, 200)]],
        ..Obstacles::default()
    };
    let points = run(&obstacles, (0, 0), (500, 400));
    for q in &points[1..points.len() - 1] {
        assert_ne!(q.1, 200, "a corner on the Wire: {points:?}");
    }
}

#[test]
fn an_angled_wire_counts_as_a_crossing() {
    let obstacles = Obstacles {
        wires: vec![vec![p(150, -500), p(350, 500)]],
        ..Obstacles::default()
    };
    let route = route(&obstacles, p(0, 0), p(500, 0), &options()).unwrap();
    assert_eq!(mil(&route.points), [(0, 0), (500, 0)]);
    assert_eq!(route.crossings, 1);
}

#[test]
fn a_route_may_start_on_a_wire_end_and_end_on_a_wire() {
    let obstacles = Obstacles {
        wires: vec![vec![p(-300, 0), p(0, 0)], vec![p(500, -300), p(500, 300)]],
        ..Obstacles::default()
    };
    let route = route(&obstacles, p(0, 0), p(500, 0), &options()).unwrap();
    assert_eq!(mil(&route.points), [(0, 0), (500, 0)]);
    assert_eq!(route.crossings, 0);
}

#[test]
fn off_grid_ends_are_kept_exactly() {
    let points = run(&Obstacles::default(), (13, 7), (512, 333));
    assert_eq!(points.first(), Some(&(13, 7)));
    assert_eq!(points.last(), Some(&(512, 333)));
}

#[test]
fn a_walled_in_end_has_no_route() {
    let obstacles = Obstacles {
        boxes: vec![
            body(400, -100, 600, -100),
            body(400, 100, 600, 100),
            body(400, -100, 400, 100),
            body(600, -100, 600, 100),
        ],
        ..Obstacles::default()
    };
    assert_eq!(
        route(&obstacles, p(0, 0), p(500, 0), &options()),
        Err(RouteError::NoRoute)
    );
}

#[test]
fn time_out_after_bounds_the_search() {
    let obstacles = Obstacles {
        boxes: vec![
            body(400, -100, 600, -100),
            body(400, 100, 600, 100),
            body(400, -100, 400, 100),
            body(600, -100, 600, 100),
        ],
        ..Obstacles::default()
    };
    let options = Options {
        grid: Nm::from_mil(1),
        timeout_ms: 1,
        ..options()
    };
    assert_eq!(
        route(&obstacles, p(0, 0), p(500, 0), &options),
        Err(RouteError::TimedOut)
    );
}

#[test]
fn the_same_inputs_give_the_same_route() {
    let obstacles = Obstacles {
        boxes: vec![body(100, 100, 300, 300), body(-300, 200, -100, 400)],
        points: vec![p(200, 0), p(0, 250)],
        wires: vec![
            vec![p(-500, 150), p(800, 150)],
            vec![p(50, -400), p(450, 600)],
        ],
    };
    let first = route(&obstacles, p(0, 0), p(600, 500), &options()).unwrap();
    for _ in 0..5 {
        assert_eq!(
            route(&obstacles, p(0, 0), p(600, 500), &options()).unwrap(),
            first
        );
    }
    // The reversed obstacle lists give the same route too.
    let mut reversed = obstacles.clone();
    reversed.boxes.reverse();
    reversed.points.reverse();
    reversed.wires.reverse();
    assert_eq!(
        route(&reversed, p(0, 0), p(600, 500), &options()).unwrap(),
        first
    );
}
