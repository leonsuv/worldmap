//! Small geographic helpers shared by routes and background tasks.

pub const EARTH_RADIUS_KM: f64 = 6371.0088;

/// Great-circle distance in kilometres.
pub fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2) + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    EARTH_RADIUS_KM * 2.0 * a.sqrt().min(1.0).asin()
}

/// Wrap any longitude into [-180, 180).
pub fn wrap_lon(lon: f64) -> f64 {
    (lon + 180.0).rem_euclid(360.0) - 180.0
}

pub fn valid_lat_lon(lat: f64, lon: f64) -> bool {
    lat.is_finite() && lon.is_finite() && lat.abs() <= 90.0 && lon.abs() <= 180.0
}

/// A viewport as sent by MapLibre: west may exceed east after wrapping, and
/// longitudes may lie outside [-180, 180] when the world is repeated.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BBox {
    pub west: f64,
    pub south: f64,
    pub east: f64,
    pub north: f64,
}

impl BBox {
    /// Parse `west,south,east,north`.
    pub fn parse(value: &str) -> Option<Self> {
        let parts: Vec<f64> = value.split(',').map(|s| s.trim().parse::<f64>()).collect::<Result<_, _>>().ok()?;
        let [west, south, east, north] = parts[..] else { return None };
        if parts.iter().any(|v| !v.is_finite()) || south < -90.0 || north > 90.0 || south > north {
            return None;
        }
        if (east - west).abs() > 1080.0 {
            return None;
        }
        Some(Self { west, south, east, north })
    }

    /// Longitudinal extent in degrees, 0..=360.
    pub fn lon_span(&self) -> f64 {
        let span = self.east - self.west;
        if span >= 360.0 {
            360.0
        } else {
            span.rem_euclid(360.0)
        }
    }

    pub fn contains(&self, lat: f64, lon: f64) -> bool {
        if lat < self.south || lat > self.north {
            return false;
        }
        let span = self.lon_span();
        if span >= 360.0 {
            return true;
        }
        (lon - self.west).rem_euclid(360.0) <= span
    }

    pub fn center(&self) -> (f64, f64) {
        ((self.south + self.north) / 2.0, wrap_lon(self.west + self.lon_span() / 2.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distances_are_plausible() {
        let berlin_paris = haversine_km(52.52, 13.405, 48.8566, 2.3522);
        assert!((berlin_paris - 878.0).abs() < 5.0, "{berlin_paris}");
        assert!(haversine_km(10.0, 179.9, 10.0, -179.9) < 25.0);
        assert_eq!(haversine_km(1.0, 2.0, 1.0, 2.0), 0.0);
    }

    #[test]
    fn bbox_handles_wrapping_and_rejects_bad_input() {
        for bad in ["", "1,2,3", "x,1,2,3", "0,0,NaN,1", "0,-91,1,2", "0,4,1,2", "0,0,2000,1"] {
            assert!(BBox::parse(bad).is_none(), "{bad}");
        }
        let normal = BBox::parse("10,50,14,54").unwrap();
        assert_eq!(normal.center(), (52.0, 12.0));
        assert!(normal.contains(52.0, 12.0) && !normal.contains(52.0, 15.0));

        let across = BBox::parse("170,-10,-170,10").unwrap();
        assert_eq!(across.lon_span(), 20.0);
        assert!(across.contains(0.0, 179.0) && across.contains(0.0, -175.0) && !across.contains(0.0, 0.0));
        assert_eq!(across.center(), (0.0, -180.0));

        let repeated = BBox::parse("350,50,370,54").unwrap();
        assert_eq!(repeated.center(), (52.0, 0.0));
        assert!(repeated.contains(52.0, 5.0));

        let world = BBox::parse("-400,-80,400,80").unwrap();
        assert!(world.contains(0.0, 123.0));
    }

    #[test]
    fn longitudes_wrap() {
        assert_eq!(wrap_lon(190.0), -170.0);
        assert_eq!(wrap_lon(-190.0), 170.0);
        assert_eq!(wrap_lon(180.0), -180.0);
        assert_eq!(wrap_lon(12.5), 12.5);
    }
}
