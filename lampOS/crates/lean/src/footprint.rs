//! Convex support geometry in metres, independent of a hardware model.
//!
//! V1 scored radial COM offset (`recording_stability.py`); that cannot describe
//! an asymmetric support footprint. This module measures true signed Euclidean
//! distance to its inward offset, including outside corners. It does not infer
//! a convex hull from malformed input or retain edges that disappear on inset.

use crate::Error;

const MAX_VERTICES: usize = 32;
// Intersecting at most 32 original and 32 inset half-planes has at most 64
// vertices; two temporary crossing duplicates fit before simplification.
const MAX_WORK_VERTICES: usize = 2 * MAX_VERTICES + 2;
const MIN_EDGE_M: f64 = 1e-6;
// At the accepted +/-2 m coordinate scale this is above rounding noise. Input
// turns within this perpendicular distance of collinearity are rejected. Inset
// vertices within it of a neighbouring chord are removed, conservatively
// shrinking the polygon. Callers require clearance > 1e-9 m, not this tolerance,
// to pass; neither tolerance represents uncertainty in real lamp measurements.
const GEOMETRY_EPS_M: f64 = 1e-12;

type Point = [f64; 2];

#[derive(Clone, Debug)]
pub(crate) struct Footprint {
    vertices: Vec<Point>,
}

#[derive(Clone, Copy)]
struct Plane {
    origin: Point,
    inward: Point,
}

impl Plane {
    fn distance(self, point: Point) -> f64 {
        dot(self.inward, sub(point, self.origin))
    }
}

impl Footprint {
    pub(crate) fn new(vertices: &[Point], margin_m: f64) -> Result<Self, Error> {
        if !(3..=MAX_VERTICES).contains(&vertices.len()) {
            return Err(Error("footprint needs 3..32 vertices".into()));
        }
        if vertices
            .iter()
            .flatten()
            .any(|v| !v.is_finite() || v.abs() > 2.0)
        {
            return Err(Error(
                "footprint coordinates must be finite in -2..2 m".into(),
            ));
        }
        if !margin_m.is_finite() || !(0.0..=1.0).contains(&margin_m) {
            return Err(Error("footprint margin must be finite in 0..1 m".into()));
        }
        let planes = convex_planes(vertices, MIN_EDGE_M)?;
        let mut inset = vertices.to_vec();
        if margin_m != 0.0 {
            for plane in &planes {
                inset = clip(&inset, *plane, margin_m)?;
                simplify(&mut inset);
                if inset.len() < 3 {
                    return Err(Error(
                        "footprint margin leaves no nondegenerate polygon".into(),
                    ));
                }
            }
        }
        if inset.len() > MAX_VERTICES {
            return Err(Error("footprint inset exceeded its vertex bound".into()));
        }
        convex_planes(&inset, GEOMETRY_EPS_M)?;
        if inset.iter().any(|point| {
            planes
                .iter()
                .any(|plane| plane.distance(*point) - margin_m < -GEOMETRY_EPS_M)
        }) {
            return Err(Error("footprint inset exceeded numerical tolerance".into()));
        }
        Ok(Self { vertices: inset })
    }

    /// Signed distance to the inset boundary: positive inside, negative outside.
    /// A nonfinite query returns NaN, which cannot pass a positive-clearance gate.
    pub(crate) fn clearance(&self, point: Point) -> f64 {
        if point.iter().any(|v| !v.is_finite()) {
            return f64::NAN;
        }
        let mut inside = true;
        let mut distance = f64::INFINITY;
        for i in 0..self.vertices.len() {
            let a = self.vertices[i];
            let b = self.vertices[(i + 1) % self.vertices.len()];
            let edge = sub(b, a);
            let length = norm(edge);
            let inward = [-edge[1] / length, edge[0] / length];
            inside &= dot(inward, sub(point, a)) >= 0.0;
            distance = distance.min(segment_distance(point, a, b));
        }
        if inside { distance } else { -distance }
    }

    pub(crate) fn vertices(&self) -> &[Point] {
        &self.vertices
    }
}

fn convex_planes(vertices: &[Point], min_edge_m: f64) -> Result<Vec<Plane>, Error> {
    let mut planes = Vec::with_capacity(vertices.len());
    for i in 0..vertices.len() {
        let next = (i + 1) % vertices.len();
        let edge = sub(vertices[next], vertices[i]);
        let length = norm(edge);
        if length < min_edge_m || length == 0.0 {
            return Err(Error("footprint has a duplicate or too-short edge".into()));
        }
        let plane = Plane {
            origin: vertices[i],
            inward: [-edge[1] / length, edge[0] / length],
        };
        // Local turn tests alone admit some self-crossing polygons. Every other
        // vertex must lie strictly left of every directed edge instead.
        if vertices
            .iter()
            .enumerate()
            .any(|(j, point)| j != i && j != next && plane.distance(*point) <= GEOMETRY_EPS_M)
        {
            return Err(Error(
                "footprint must be strictly convex and counterclockwise".into(),
            ));
        }
        planes.push(plane);
    }
    Ok(planes)
}

fn clip(vertices: &[Point], plane: Plane, margin_m: f64) -> Result<Vec<Point>, Error> {
    let mut output = Vec::with_capacity(MAX_WORK_VERTICES);
    let mut push = |point| {
        if output.len() == MAX_WORK_VERTICES {
            return Err(Error("footprint clipping exceeded its work bound".into()));
        }
        output.push(point);
        Ok(())
    };
    for i in 0..vertices.len() {
        let a = vertices[i];
        let b = vertices[(i + 1) % vertices.len()];
        let da = plane.distance(a) - margin_m;
        let db = plane.distance(b) - margin_m;
        if (da >= 0.0) != (db >= 0.0) {
            // Opposite signs make the denominator nonzero and keep the ratio
            // in [0,1]. Do not expand a half-plane by a classification epsilon.
            let share = (da / (da - db)).clamp(0.0, 1.0);
            push([a[0] + share * (b[0] - a[0]), a[1] + share * (b[1] - a[1])])?;
        }
        if db >= 0.0 {
            push(b)?;
        }
    }
    Ok(output)
}

fn simplify(vertices: &mut Vec<Point>) {
    loop {
        if vertices.len() < 3 {
            return;
        }
        let count = vertices.len();
        let redundant = (0..count).find(|&i| {
            segment_distance(
                vertices[i],
                vertices[(i + count - 1) % count],
                vertices[(i + 1) % count],
            ) <= GEOMETRY_EPS_M
        });
        match redundant {
            Some(i) => {
                // A chord between convex vertices stays inside the polygon.
                vertices.remove(i);
            }
            None => return,
        }
    }
}

fn segment_distance(point: Point, a: Point, b: Point) -> f64 {
    let edge = sub(b, a);
    let length = norm(edge);
    if length == 0.0 {
        return norm(sub(point, a));
    }
    let tangent = [edge[0] / length, edge[1] / length];
    let along = dot(sub(point, a), tangent).clamp(0.0, length);
    norm(sub(
        point,
        [a[0] + along * tangent[0], a[1] + along * tangent[1]],
    ))
}

fn sub(a: Point, b: Point) -> Point {
    [a[0] - b[0], a[1] - b[1]]
}

fn dot(a: Point, b: Point) -> f64 {
    a[0].mul_add(b[0], a[1] * b[1])
}

fn norm(point: Point) -> f64 {
    point[0].hypot(point[1])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rectangle(x: f64, y: f64) -> [Point; 4] {
        [[-x, -y], [x, -y], [x, y], [-x, y]]
    }

    fn near(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-11,
            "actual {actual}, expected {expected}"
        );
    }

    #[test]
    fn true_distance_to_inset_including_corners() {
        let shape = Footprint::new(&rectangle(0.1, 0.1), 0.01).unwrap();
        near(shape.clearance([0.0, 0.0]), 0.09);
        near(shape.clearance([0.09, 0.0]), 0.0);
        near(shape.clearance([0.1, 0.0]), -0.01);
        near(shape.clearance([0.095, 0.095]), -0.005 * 2f64.sqrt());
        near(shape.clearance([0.13, 0.14]), -0.04f64.hypot(0.05));
        // Half-plane slack (-.05) and physical distance minus margin (-.06)
        // would both return the wrong outside-corner distance.
        assert!(shape.clearance([0.13, 0.14]) < -0.064);
    }

    #[test]
    fn asymmetric_support_distinguishes_yaw_and_rotates_with_coordinates() {
        let vertices = rectangle(0.1, 0.05);
        let shape = Footprint::new(&vertices, 0.01).unwrap();
        near(shape.clearance([0.08, 0.0]), 0.01);
        near(shape.clearance([0.0, 0.08]), -0.04);
        let rotated = vertices.map(|p| [-p[1], p[0]]);
        let rotated_shape = Footprint::new(&rotated, 0.01).unwrap();
        near(rotated_shape.clearance([0.0, 0.08]), 0.01);
    }

    #[test]
    fn inset_removes_a_short_chamfer_edge() {
        let pentagon = [
            [-1.0, -1.0],
            [1.0, -1.0],
            [1.0, 0.9],
            [0.9, 1.0],
            [-1.0, 1.0],
        ];
        assert_eq!(Footprint::new(&pentagon, 0.16).unwrap().vertices().len(), 5);
        let shape = Footprint::new(&pentagon, 0.2).unwrap();
        assert_eq!(shape.vertices().len(), 4);
        for point in shape.vertices() {
            near(point[0].abs(), 0.8);
            near(point[1].abs(), 0.8);
        }
        near(shape.clearance([0.0, 0.0]), 0.8);
        near(shape.clearance([1.0, 1.0]), -0.2 * 2f64.sqrt());
        let disappearance = 0.1 / (2.0 - 2f64.sqrt());
        let at_disappearance = Footprint::new(&pentagon, disappearance).unwrap();
        assert_eq!(at_disappearance.vertices().len(), 4);
        near(at_disappearance.clearance([0.0, 0.0]), 1.0 - disappearance);
        assert!(Footprint::new(&pentagon, 1.0).is_err());
    }

    #[test]
    fn maximum_vertex_count_retains_the_analytic_regular_polygon() {
        let vertices: Vec<_> = (0..MAX_VERTICES)
            .map(|i| {
                let angle = i as f64 * std::f64::consts::TAU / MAX_VERTICES as f64;
                [angle.cos(), angle.sin()]
            })
            .collect();
        let shape = Footprint::new(&vertices, 0.1).unwrap();
        let apothem = (std::f64::consts::PI / MAX_VERTICES as f64).cos();
        assert_eq!(shape.vertices().len(), MAX_VERTICES);
        near(shape.clearance([0.0, 0.0]), apothem - 0.1);
        near(shape.clearance([2.0, 0.0]), -1.0 - 0.1 / apothem);
    }

    #[test]
    fn diagonal_inset_uses_unit_normals() {
        let shape = Footprint::new(&[[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]], 0.1).unwrap();
        near(shape.clearance([0.2, 0.2]), 0.1);
        near(shape.clearance([0.0, 0.0]), -0.1 * 2f64.sqrt());
        near(shape.clearance([0.5, 0.5]), -0.1);
        let far_corner = 0.9 - 0.1 * 2f64.sqrt();
        assert!(
            shape
                .vertices()
                .iter()
                .any(|p| { (p[0] - far_corner).abs() < 1e-12 && (p[1] - 0.1).abs() < 1e-12 })
        );
    }

    #[test]
    fn zero_margin_and_maximum_declared_scale() {
        let vertices = rectangle(2.0, 2.0);
        let original = Footprint::new(&vertices, 0.0).unwrap();
        assert_eq!(original.vertices(), &vertices);
        near(original.clearance([3.0, 3.0]), -2f64.sqrt());
        near(
            Footprint::new(&vertices, 1.0)
                .unwrap()
                .clearance([0.0, 0.0]),
            1.0,
        );
    }

    #[test]
    fn rejects_empty_line_point_and_numerically_degenerate_insets() {
        let square = rectangle(0.1, 0.1);
        assert!(Footprint::new(&square, 0.11).is_err());
        assert!(Footprint::new(&square, 0.1).is_err());
        assert!(Footprint::new(&rectangle(0.2, 0.1), 0.1).is_err());
        assert!(Footprint::new(&rectangle(1e-6, 1e-6), 1e-6 - 1e-13).is_err());
    }

    #[test]
    fn refuses_concave_crossed_clockwise_and_collinear_input() {
        let bad = [
            vec![[0.0, 0.0], [1.0, 0.0], [0.2, 0.2], [0.0, 1.0]],
            vec![[0.0, 0.0], [1.0, 1.0], [0.0, 1.0], [1.0, 0.0]],
            vec![[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]],
            vec![[0.0, 0.0], [0.5, 0.0], [1.0, 0.0], [0.0, 1.0]],
            vec![[0.0, 0.0], [1.0, 0.0], [0.5, GEOMETRY_EPS_M / 2.0]],
        ];
        for vertices in bad {
            assert!(Footprint::new(&vertices, 0.0).is_err());
        }
        // A pentagram can have consistently signed local turns; checking all
        // vertices against every edge is necessary to refuse it.
        let star: Vec<_> = [0, 2, 4, 1, 3]
            .into_iter()
            .map(|i| {
                let angle = i as f64 * std::f64::consts::TAU / 5.0;
                [angle.cos(), angle.sin()]
            })
            .collect();
        assert!(Footprint::new(&star, 0.0).is_err());
    }

    #[test]
    fn refuses_duplicates_short_edges_and_unbounded_input() {
        let square = rectangle(0.1, 0.1);
        let mut closed = square.to_vec();
        closed.push(square[0]);
        assert!(Footprint::new(&closed, 0.0).is_err());
        let mut duplicate = square.to_vec();
        duplicate[2] = duplicate[0];
        assert!(Footprint::new(&duplicate, 0.0).is_err());
        assert!(Footprint::new(&[[0.0, 0.0], [1e-7, 0.0], [0.0, 1.0]], 0.0).is_err());
        assert!(Footprint::new(&square[..2], 0.0).is_err());
        assert!(Footprint::new(&[[0.0, 0.0]; 33], 0.0).is_err());
        assert!(Footprint::new(&rectangle(2.001, 1.0), 0.0).is_err());
        for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            for axis in 0..2 {
                let mut vertices = square;
                vertices[1][axis] = invalid;
                assert!(Footprint::new(&vertices, 0.0).is_err());
            }
        }
        for invalid in [-0.1, 1.001, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(Footprint::new(&square, invalid).is_err());
        }
    }

    #[test]
    fn invalid_queries_cannot_report_positive_clearance() {
        let shape = Footprint::new(&rectangle(0.1, 0.1), 0.01).unwrap();
        for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(shape.clearance([invalid, 0.0]).is_nan());
            assert!(shape.clearance([0.0, invalid]).is_nan());
        }
        assert!(shape.clearance([f64::MAX, f64::MAX]) < 0.0);
        assert!(shape.clearance([-f64::MAX, f64::MAX]) < 0.0);
    }
}
