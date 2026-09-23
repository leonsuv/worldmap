//! Geometry operations in Web Mercator unit space (x, y ∈ [0, 1], y down).

pub type P = [f64; 2];

pub const MAX_LAT: f64 = 85.051_128_779_806_59;

/// Project WGS84 longitude/latitude into Web Mercator unit space.
pub fn project(lon: f64, lat: f64) -> P {
    let lat = lat.clamp(-MAX_LAT, MAX_LAT).to_radians();
    let x = (lon + 180.0) / 360.0;
    let y = (1.0 - (lat.tan() + 1.0 / lat.cos()).ln() / std::f64::consts::PI) / 2.0;
    [x.clamp(0.0, 1.0), y.clamp(0.0, 1.0)]
}

/// Inverse of [`project`].
pub fn unproject(p: P) -> (f64, f64) {
    let lon = p[0] * 360.0 - 180.0;
    let n = std::f64::consts::PI * (1.0 - 2.0 * p[1]);
    (lon, n.sinh().atan().to_degrees())
}

/// Web Mercator metres (EPSG:3857) to longitude/latitude.
pub fn mercator_meters_to_lon_lat(x: f64, y: f64) -> (f64, f64) {
    const R: f64 = 6_378_137.0;
    let lon = (x / R).to_degrees();
    let lat = (2.0 * (y / R).exp().atan() - std::f64::consts::FRAC_PI_2).to_degrees();
    (lon, lat)
}

pub fn length(line: &[P]) -> f64 {
    line.windows(2).map(|w| ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2)).sqrt()).sum()
}

/// Signed area (surveyor's formula); positive = clockwise with y down.
pub fn signed_area(ring: &[P]) -> f64 {
    let n = ring.len();
    if n < 3 {
        return 0.0;
    }
    let mut sum = 0.0;
    for i in 0..n {
        let a = ring[i];
        let b = ring[(i + 1) % n];
        sum += a[0] * b[1] - b[0] * a[1];
    }
    sum / 2.0
}

fn seg_dist_sq(p: P, a: P, b: P) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len_sq = dx * dx + dy * dy;
    let t = if len_sq == 0.0 { 0.0 } else { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len_sq).clamp(0.0, 1.0) };
    let (x, y) = (a[0] + t * dx, a[1] + t * dy);
    (p[0] - x).powi(2) + (p[1] - y).powi(2)
}

/// Douglas–Peucker simplification keeping both end points.
pub fn simplify(points: &[P], tolerance: f64) -> Vec<P> {
    if points.len() <= 2 || tolerance <= 0.0 {
        return points.to_vec();
    }
    let tol_sq = tolerance * tolerance;
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;
    let mut stack = vec![(0usize, points.len() - 1)];
    while let Some((first, last)) = stack.pop() {
        let mut max_d = 0.0;
        let mut index = 0;
        for i in first + 1..last {
            let d = seg_dist_sq(points[i], points[first], points[last]);
            if d > max_d {
                max_d = d;
                index = i;
            }
        }
        if max_d > tol_sq {
            keep[index] = true;
            stack.push((first, index));
            stack.push((index, last));
        }
    }
    points.iter().zip(keep).filter_map(|(p, k)| k.then_some(*p)).collect()
}

/// Axis-aligned rectangle `[min_x, min_y, max_x, max_y]`.
pub type Rect = [f64; 4];

fn inside(p: P, r: &Rect) -> bool {
    p[0] >= r[0] && p[0] <= r[2] && p[1] >= r[1] && p[1] <= r[3]
}

/// Liang–Barsky: the part of segment a→b inside `r` as (start, end, t_end).
fn clip_segment(a: P, b: P, r: &Rect) -> Option<(P, P, f64)> {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let mut t0: f64 = 0.0;
    let mut t1: f64 = 1.0;
    for (p, q) in [(-dx, a[0] - r[0]), (dx, r[2] - a[0]), (-dy, a[1] - r[1]), (dy, r[3] - a[1])] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
            if t0 > t1 {
                return None;
            }
        }
    }
    // Clamp so rounding never places an intersection a hair outside the rectangle.
    let at = |t: f64| [(a[0] + t * dx).clamp(r[0], r[2]), (a[1] + t * dy).clamp(r[1], r[3])];
    Some((if t0 == 0.0 { a } else { at(t0) }, if t1 == 1.0 { b } else { at(t1) }, t1))
}

/// Clip a line string to a rectangle; a line that leaves and re-enters
/// becomes several parts.
pub fn clip_line(points: &[P], r: &Rect) -> Vec<Vec<P>> {
    let mut parts = Vec::new();
    let mut current: Vec<P> = Vec::new();
    for w in points.windows(2) {
        match clip_segment(w[0], w[1], r) {
            None => {
                if current.len() >= 2 {
                    parts.push(std::mem::take(&mut current));
                }
                current.clear();
            }
            Some((start, end, t_end)) => {
                if current.last() != Some(&start) {
                    if current.len() >= 2 {
                        parts.push(std::mem::take(&mut current));
                    }
                    current = vec![start];
                }
                current.push(end);
                if t_end < 1.0 {
                    if current.len() >= 2 {
                        parts.push(std::mem::take(&mut current));
                    }
                    current.clear();
                }
            }
        }
    }
    if current.len() >= 2 {
        parts.push(current);
    }
    parts
}

/// Sutherland–Hodgman clipping of one ring (without closing point).
pub fn clip_ring(ring: &[P], r: &Rect) -> Vec<P> {
    let mut output = ring.to_vec();
    for edge in 0..4 {
        if output.is_empty() {
            break;
        }
        let input = std::mem::take(&mut output);
        let is_in = |p: &P| match edge {
            0 => p[0] >= r[0],
            1 => p[0] <= r[2],
            2 => p[1] >= r[1],
            _ => p[1] <= r[3],
        };
        let intersect = |a: &P, b: &P| -> P {
            match edge {
                0 | 1 => {
                    let x = if edge == 0 { r[0] } else { r[2] };
                    let t = (x - a[0]) / (b[0] - a[0]);
                    [x, a[1] + t * (b[1] - a[1])]
                }
                _ => {
                    let y = if edge == 2 { r[1] } else { r[3] };
                    let t = (y - a[1]) / (b[1] - a[1]);
                    [a[0] + t * (b[0] - a[0]), y]
                }
            }
        };
        let mut prev = input[input.len() - 1];
        for cur in input {
            match (is_in(&cur), is_in(&prev)) {
                (true, true) => output.push(cur),
                (true, false) => {
                    output.push(intersect(&prev, &cur));
                    output.push(cur);
                }
                (false, true) => output.push(intersect(&prev, &cur)),
                (false, false) => {}
            }
            prev = cur;
        }
    }
    output
}

pub fn points_in(points: &[P], r: &Rect) -> Vec<P> {
    points.iter().copied().filter(|p| inside(*p, r)).collect()
}

/// Split a lon/lat line wherever it jumps across the antimeridian.
pub fn split_antimeridian(coords: Vec<(f64, f64)>) -> Vec<Vec<(f64, f64)>> {
    let mut parts = Vec::new();
    let mut current: Vec<(f64, f64)> = Vec::new();
    for c in coords {
        if let Some(last) = current.last() {
            if (c.0 - last.0).abs() > 180.0 {
                if current.len() >= 2 {
                    parts.push(std::mem::take(&mut current));
                }
                current.clear();
            }
        }
        current.push(c);
    }
    if current.len() >= 2 {
        parts.push(current);
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_round_trips() {
        for (lon, lat) in [(0.0, 0.0), (13.4, 52.5), (-122.4, 37.8), (179.9, -60.0)] {
            let (lo, la) = unproject(project(lon, lat));
            assert!((lo - lon).abs() < 1e-9 && (la - lat).abs() < 1e-9, "{lon},{lat}");
        }
        let corner = project(-180.0, MAX_LAT);
        assert!(corner[0] == 0.0 && corner[1].abs() < 1e-12, "{corner:?}");
        let (lon, lat) = mercator_meters_to_lon_lat(1_491_681.0, 6_894_701.0);
        assert!((lon - 13.4).abs() < 0.01 && (lat - 52.52).abs() < 0.01, "{lon},{lat}");
    }

    #[test]
    fn simplification_keeps_shape_and_ends() {
        let line: Vec<P> = (0..=100).map(|i| [i as f64 / 100.0, if i == 50 { 0.5 } else { 0.0 }]).collect();
        let s = simplify(&line, 0.01);
        assert_eq!(s, vec![[0.0, 0.0], [0.49, 0.0], [0.5, 0.5], [0.51, 0.0], [1.0, 0.0]]);
        assert_eq!(simplify(&line[..2], 1.0).len(), 2);
    }

    #[test]
    fn lines_leaving_and_reentering_are_split() {
        let r = [0.0, 0.0, 1.0, 1.0];
        let line = vec![[-1.0, 0.5], [0.5, 0.5], [0.5, 2.0], [0.7, 2.0], [0.7, 0.5], [2.0, 0.5]];
        let parts = clip_line(&line, &r);
        assert_eq!(parts, vec![vec![[0.0, 0.5], [0.5, 0.5], [0.5, 1.0]], vec![[0.7, 1.0], [0.7, 0.5], [1.0, 0.5]]]);
        assert!(clip_line(&[[2.0, 2.0], [3.0, 3.0]], &r).is_empty());
        assert_eq!(clip_line(&[[0.1, 0.1], [0.2, 0.2], [0.3, 0.1]], &r).len(), 1);
    }

    #[test]
    fn rings_are_clipped_to_the_rectangle() {
        let square = vec![[-1.0, -1.0], [2.0, -1.0], [2.0, 2.0], [-1.0, 2.0]];
        let clipped = clip_ring(&square, &[0.0, 0.0, 1.0, 1.0]);
        assert!((signed_area(&clipped).abs() - 1.0).abs() < 1e-12, "{clipped:?}");
        assert!(clip_ring(&[[5.0, 5.0], [6.0, 5.0], [6.0, 6.0]], &[0.0, 0.0, 1.0, 1.0]).is_empty());
    }

    #[test]
    fn antimeridian_jumps_split_lines() {
        let parts = split_antimeridian(vec![(178.0, 60.0), (179.5, 60.0), (-179.5, 60.1), (-178.0, 60.2)]);
        assert_eq!(parts.len(), 2);
        assert!(split_antimeridian(vec![(1.0, 1.0)]).is_empty());
    }
}
