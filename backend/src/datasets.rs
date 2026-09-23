//! Imported static datasets (airports, seaports, nuclear plants), held in
//! memory with pre-serialised, pre-compressed GeoJSON responses.

use axum::http::{header, HeaderMap};
use axum::response::{IntoResponse, Response};
use rusqlite::Connection;
use serde::Serialize;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;
use std::time::SystemTime;

use crate::db::table_columns;

/// JSON serialised once and gzip-compressed once.
pub struct StaticBody {
    identity: axum::body::Bytes,
    gzip: axum::body::Bytes,
}

impl StaticBody {
    pub fn new(value: &impl Serialize) -> anyhow::Result<Self> {
        let identity = serde_json::to_vec(value)?;
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::new(6));
        encoder.write_all(&identity)?;
        Ok(Self { identity: identity.into(), gzip: encoder.finish()?.into() })
    }

    pub fn response(&self, headers: &HeaderMap, max_age: u32) -> Response {
        let cache = format!("public, max-age={max_age}");
        if accepts_gzip(headers) {
            (
                [
                    (header::CONTENT_TYPE, "application/geo+json".to_string()),
                    (header::CACHE_CONTROL, cache),
                    (header::VARY, "Accept-Encoding".to_string()),
                    (header::CONTENT_ENCODING, "gzip".to_string()),
                ],
                self.gzip.clone(),
            )
                .into_response()
        } else {
            (
                [
                    (header::CONTENT_TYPE, "application/geo+json".to_string()),
                    (header::CACHE_CONTROL, cache),
                    (header::VARY, "Accept-Encoding".to_string()),
                ],
                self.identity.clone(),
            )
                .into_response()
        }
    }

    pub fn identity_len(&self) -> usize {
        self.identity.len()
    }
}

pub fn accepts_gzip(headers: &HeaderMap) -> bool {
    let mut wildcard = false;
    for value in headers.get_all(header::ACCEPT_ENCODING) {
        let Ok(value) = value.to_str() else { continue };
        for encoding in value.split(',') {
            let mut parts = encoding.trim().split(';');
            let name = parts.next().unwrap_or("").trim();
            let mut quality = 1.0_f32;
            for param in parts {
                if let Some((key, value)) = param.trim().split_once('=') {
                    if key.trim().eq_ignore_ascii_case("q") {
                        quality = value.trim().parse().unwrap_or(0.0);
                    }
                }
            }
            if name.eq_ignore_ascii_case("gzip") {
                return quality > 0.0;
            }
            if name == "*" {
                wildcard = quality > 0.0;
            }
        }
    }
    wildcard
}

#[derive(Clone, Debug, Serialize)]
pub struct Airport {
    pub ident: String,
    pub iata: Option<String>,
    pub name: String,
    pub city: Option<String>,
    pub country: Option<String>,
    /// `large`, `medium` or `small`.
    pub kind: String,
    pub lat: f64,
    pub lon: f64,
    pub elevation_ft: Option<f64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Seaport {
    pub name: String,
    pub locode: Option<String>,
    pub country: Option<String>,
    /// `large`, `medium`, `small`, `very_small` or null.
    pub size: Option<String>,
    pub harbor_type: Option<String>,
    pub lat: f64,
    pub lon: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct ReactorUnit {
    pub name: String,
    pub status: String,
    pub capacity_mw: Option<f64>,
    pub reactor_type: Option<String>,
    pub model: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct NuclearPlant {
    pub name: String,
    pub country: String,
    pub lat: f64,
    pub lon: f64,
    /// Net capacity of operating units.
    pub capacity_mw: f64,
    /// `operational`, `construction` or `suspended`.
    pub status: String,
    pub units_operational: usize,
    pub units_construction: usize,
    pub units: Vec<ReactorUnit>,
}

#[derive(Default)]
pub struct Datasets {
    pub airports: Vec<Airport>,
    pub seaports: Vec<Seaport>,
    pub plants: Vec<NuclearPlant>,
    pub airports_body: Option<StaticBody>,
    pub seaports_body: Option<StaticBody>,
    pub plants_body: Option<StaticBody>,
    pub loaded_from: Option<SystemTime>,
}

#[derive(Serialize)]
struct Feature<'a, P: Serialize> {
    r#type: &'static str,
    geometry: Point,
    properties: &'a P,
}

#[derive(Serialize)]
struct Point {
    r#type: &'static str,
    coordinates: [f64; 2],
}

#[derive(Serialize)]
struct Collection<'a, P: Serialize> {
    r#type: &'static str,
    features: Vec<Feature<'a, P>>,
}

fn collection<'a, P: Serialize>(items: &'a [P], coords: impl Fn(&P) -> (f64, f64)) -> Collection<'a, P> {
    Collection {
        r#type: "FeatureCollection",
        features: items
            .iter()
            .map(|p| {
                let (lat, lon) = coords(p);
                Feature { r#type: "Feature", geometry: Point { r#type: "Point", coordinates: [lon, lat] }, properties: p }
            })
            .collect(),
    }
}

/// Modification time of a SQLite database including its WAL file.
pub fn db_mtime(path: &Path) -> Option<SystemTime> {
    let main = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    let wal = std::fs::metadata(path.with_extension("db-wal")).and_then(|m| m.modified()).ok();
    main.max(wal)
}

impl Datasets {
    pub fn load(path: &Path) -> Self {
        let loaded_from = db_mtime(path);
        let conn = match Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY) {
            Ok(conn) => conn,
            Err(_) => {
                tracing::warn!("No static dataset database at {} — run the import (make ingest)", path.display());
                return Self { loaded_from, ..Self::default() };
            }
        };
        let airports = load_airports(&conn);
        let seaports = load_seaports(&conn);
        let plants = load_plants(&conn);
        tracing::info!("Datasets: {} airports, {} seaports, {} nuclear plants", airports.len(), seaports.len(), plants.len());
        fn body(value: &impl Serialize) -> Option<StaticBody> {
            StaticBody::new(value).map_err(|e| tracing::error!("Could not serialise dataset: {e}")).ok()
        }
        Self {
            airports_body: body(&collection(&airports, |a| (a.lat, a.lon))),
            seaports_body: body(&collection(&seaports, |s| (s.lat, s.lon))),
            plants_body: body(&collection(&plants, |p| (p.lat, p.lon))),
            airports,
            seaports,
            plants,
            loaded_from,
        }
    }
}

fn non_empty(v: Option<String>) -> Option<String> {
    v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn load_airports(conn: &Connection) -> Vec<Airport> {
    let cols = table_columns(conn, "airports");
    if cols.is_empty() {
        return Vec::new();
    }
    let pick = |name: &str, fallback: &str| if cols.contains(name) { name.to_string() } else { fallback.to_string() };
    let sql = format!(
        "SELECT {}, {}, name, city, country, {}, lat, lon, elevation_ft FROM airports",
        pick("ident", "icao"),
        pick("iata", "NULL"),
        pick("kind", "'medium'"),
    );
    let Ok(mut stmt) = conn.prepare(&sql) else { return Vec::new() };
    let rows = stmt.query_map([], |row| {
        Ok(Airport {
            ident: row.get::<_, Option<String>>(0)?.unwrap_or_default(),
            iata: non_empty(row.get(1)?),
            name: row.get(2)?,
            city: non_empty(row.get(3)?),
            country: non_empty(row.get(4)?),
            kind: row.get::<_, Option<String>>(5)?.unwrap_or_else(|| "medium".into()),
            lat: row.get(6)?,
            lon: row.get(7)?,
            elevation_ft: row.get(8)?,
        })
    });
    let mut out: Vec<Airport> = rows.map(|r| r.filter_map(Result::ok).collect()).unwrap_or_default();
    // Large airports first so they draw on top of smaller ones.
    out.sort_by_key(|a| match a.kind.as_str() {
        "large" => 2,
        "medium" => 1,
        _ => 0,
    });
    out
}

fn load_seaports(conn: &Connection) -> Vec<Seaport> {
    let cols = table_columns(conn, "seaports");
    if cols.is_empty() {
        return Vec::new();
    }
    let pick = |name: &'static str| if cols.contains(name) { name } else { "NULL" };
    let sql = format!("SELECT name, locode, country, {}, {}, lat, lon FROM seaports", pick("harbor_size"), pick("harbor_type"));
    let Ok(mut stmt) = conn.prepare(&sql) else { return Vec::new() };
    let rows = stmt.query_map([], |row| {
        Ok(Seaport {
            name: row.get(0)?,
            locode: non_empty(row.get(1)?),
            country: non_empty(row.get(2)?),
            size: non_empty(row.get(3)?),
            harbor_type: non_empty(row.get(4)?),
            lat: row.get(5)?,
            lon: row.get(6)?,
        })
    });
    let mut out: Vec<Seaport> = rows.map(|r| r.filter_map(Result::ok).collect()).unwrap_or_default();
    out.sort_by_key(|p| match p.size.as_deref() {
        Some("large") => 3,
        Some("medium") => 2,
        Some("small") => 1,
        _ => 0,
    });
    out
}

/// Plant name from a unit name: "Gravelines-5" → "Gravelines".
pub fn plant_name(unit: &str) -> String {
    let trimmed = unit.trim();
    let base = trimmed.trim_end_matches(|c: char| c.is_ascii_digit());
    if base.len() == trimmed.len() {
        return trimmed.to_string();
    }
    let base = base.trim_end_matches(['-', ' ', '_']);
    if base.is_empty() {
        trimmed.to_string()
    } else {
        base.to_string()
    }
}

fn status_class(status: &str) -> &'static str {
    let s = status.to_ascii_lowercase();
    if s.contains("construction") {
        "construction"
    } else if s.contains("suspend") {
        "suspended"
    } else {
        "operational"
    }
}

fn load_plants(conn: &Connection) -> Vec<NuclearPlant> {
    let cols = table_columns(conn, "nuclear_reactors");
    if cols.is_empty() {
        return Vec::new();
    }
    let sql = format!(
        "SELECT name, country, lat, lon, capacity_mw, status, reactor_type, {} FROM nuclear_reactors",
        if cols.contains("model") { "model" } else { "NULL" }
    );
    let Ok(mut stmt) = conn.prepare(&sql) else { return Vec::new() };
    let Ok(rows) = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?.unwrap_or_default(),
            row.get::<_, f64>(2)?,
            row.get::<_, f64>(3)?,
            row.get::<_, Option<f64>>(4)?,
            row.get::<_, Option<String>>(5)?.unwrap_or_else(|| "Operational".into()),
            non_empty(row.get(6)?),
            non_empty(row.get(7)?),
        ))
    }) else {
        return Vec::new();
    };

    // Units of one plant share a site; group by rounded location.
    let mut groups: BTreeMap<(i64, i64), Vec<_>> = BTreeMap::new();
    for row in rows.flatten() {
        let key = ((row.2 * 50.0).round() as i64, (row.3 * 50.0).round() as i64);
        groups.entry(key).or_default().push(row);
    }
    groups
        .into_values()
        .map(|units| {
            let n = units.len() as f64;
            let lat = units.iter().map(|u| u.2).sum::<f64>() / n;
            let lon = units.iter().map(|u| u.3).sum::<f64>() / n;
            let mut names: BTreeMap<String, usize> = BTreeMap::new();
            for u in &units {
                *names.entry(plant_name(&u.0)).or_default() += 1;
            }
            let name = names.into_iter().max_by_key(|(_, count)| *count).map(|(n, _)| n).unwrap_or_default();
            let country = units[0].1.clone();
            let mut unit_list: Vec<ReactorUnit> = units
                .into_iter()
                .map(|u| ReactorUnit { name: u.0, status: u.5, capacity_mw: u.4.filter(|c| *c > 0.0), reactor_type: u.6, model: u.7 })
                .collect();
            unit_list.sort_by(|a, b| a.name.cmp(&b.name));
            let classes: Vec<&str> = unit_list.iter().map(|u| status_class(&u.status)).collect();
            let units_operational = classes.iter().filter(|c| **c == "operational").count();
            let units_construction = classes.iter().filter(|c| **c == "construction").count();
            let capacity_mw = unit_list.iter().zip(&classes).filter(|(_, c)| **c == "operational").filter_map(|(u, _)| u.capacity_mw).sum();
            let status = if units_operational > 0 {
                "operational"
            } else if units_construction > 0 {
                "construction"
            } else {
                "suspended"
            };
            NuclearPlant {
                name,
                country,
                lat,
                lon,
                capacity_mw,
                status: status.into(),
                units_operational,
                units_construction,
                units: unit_list,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gzip_negotiation_honours_q_values_and_wildcards() {
        for (value, expected) in
            [("gzip, deflate, br", true), ("gzip;q=0, *;q=1", false), ("br", false), ("*;q=0.5", true), ("gzip;q=0.2", true)]
        {
            let mut headers = HeaderMap::new();
            headers.insert(header::ACCEPT_ENCODING, value.parse().unwrap());
            assert_eq!(accepts_gzip(&headers), expected, "{value}");
        }
        assert!(!accepts_gzip(&HeaderMap::new()));
    }

    #[test]
    fn precompressed_body_round_trips() {
        use std::io::Read;
        let body = StaticBody::new(&serde_json::json!({ "features": [1, 2, 3] })).unwrap();
        let mut decoded = Vec::new();
        flate2::read::GzDecoder::new(body.gzip.as_ref()).read_to_end(&mut decoded).unwrap();
        assert_eq!(decoded.as_slice(), body.identity.as_ref());
    }

    #[test]
    fn unit_names_reduce_to_plant_names() {
        assert_eq!(plant_name("Gravelines-5"), "Gravelines");
        assert_eq!(plant_name("Hinkley Point B-1"), "Hinkley Point B");
        assert_eq!(plant_name("Kursk II 1"), "Kursk II");
        assert_eq!(plant_name("Beznau"), "Beznau");
        assert_eq!(plant_name("1"), "1");
    }

    #[test]
    fn reactors_are_grouped_into_plants_and_legacy_schemas_load() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE nuclear_reactors (id INTEGER PRIMARY KEY, name TEXT NOT NULL, country TEXT NOT NULL, lat REAL NOT NULL,
               lon REAL NOT NULL, capacity_mw REAL, status TEXT, reactor_type TEXT);
             INSERT INTO nuclear_reactors (name, country, lat, lon, capacity_mw, status, reactor_type) VALUES
               ('Gravelines-1', 'France', 51.015, 2.136, 910, 'Operational', 'PWR'),
               ('Gravelines-2', 'France', 51.015, 2.136, 910, 'Operational', NULL),
               ('Flamanville-3', 'France', 49.536, -1.881, 1600, 'Under Construction', 'EPR'),
               ('Unknown', 'X', 10.0, 10.0, NULL, NULL, NULL);
             CREATE TABLE airports (id INTEGER PRIMARY KEY, icao TEXT, name TEXT NOT NULL, city TEXT, country TEXT,
               lat REAL NOT NULL, lon REAL NOT NULL, elevation_ft REAL);
             INSERT INTO airports (icao, name, city, country, lat, lon) VALUES ('EDDB', 'Berlin Brandenburg', 'Berlin', 'DE', 52.36, 13.5);",
        )
        .unwrap();
        let plants = load_plants(&conn);
        assert_eq!(plants.len(), 3);
        let gravelines = plants.iter().find(|p| p.name == "Gravelines").unwrap();
        assert_eq!(gravelines.units.len(), 2);
        assert_eq!(gravelines.capacity_mw, 1820.0);
        assert_eq!(gravelines.status, "operational");
        let flamanville = plants.iter().find(|p| p.name == "Flamanville").unwrap();
        assert_eq!((flamanville.status.as_str(), flamanville.capacity_mw), ("construction", 0.0));
        assert!(plants.iter().any(|p| p.name == "Unknown" && p.units[0].capacity_mw.is_none()));

        let airports = load_airports(&conn);
        assert_eq!(airports[0].ident, "EDDB");
        assert_eq!(airports[0].kind, "medium");
        assert!(load_seaports(&conn).is_empty());
    }
}
