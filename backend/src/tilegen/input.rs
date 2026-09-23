//! Readers for GeoJSON, GeoJSONSeq (newline-delimited) and GeoPackage.

use anyhow::{bail, Context, Result};
use rusqlite::{types::ValueRef, Connection, OpenFlags};
use serde_json::Value as Json;
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::sync::Arc;

use super::geometry::{mercator_meters_to_lon_lat, project, split_antimeridian, P};
use super::mvt::Value;

#[derive(Clone, Debug, PartialEq)]
pub enum Geometry {
    Points(Vec<P>),
    Lines(Vec<Vec<P>>),
    /// Polygons as rings without the closing point; the first ring is the exterior.
    Polygons(Vec<Vec<Vec<P>>>),
}

#[derive(Clone, Debug)]
pub struct SourceFeature {
    pub geometry: Geometry,
    pub properties: Vec<(Arc<str>, Value)>,
    pub minzoom: Option<u8>,
    pub maxzoom: Option<u8>,
}

#[derive(Default, Clone)]
pub struct InputOptions {
    /// Keep only these attributes (all when `None`).
    pub include: Option<HashSet<String>>,
    pub exclude: HashSet<String>,
    /// GeoPackage feature table (required when the file has several).
    pub gpkg_table: Option<String>,
    /// SQL condition for GeoPackage rows, e.g. `STATUS = 'OPERATING'`.
    pub where_clause: Option<String>,
}

#[derive(Default, Debug, Clone, Copy)]
pub struct ReadStats {
    pub features: u64,
    pub skipped: u64,
}

struct Interner(HashMap<String, Arc<str>>);

impl Interner {
    fn get(&mut self, key: &str) -> Arc<str> {
        if let Some(k) = self.0.get(key) {
            return k.clone();
        }
        let k: Arc<str> = Arc::from(key);
        self.0.insert(key.to_string(), k.clone());
        k
    }
}

fn keep_attr(opts: &InputOptions, key: &str) -> bool {
    !key.starts_with('_') && !opts.exclude.contains(key) && opts.include.as_ref().is_none_or(|inc| inc.contains(key))
}

// ── geometry construction from lon/lat coordinates ──────────────────────────

fn points_from(coords: impl IntoIterator<Item = (f64, f64)>) -> Vec<P> {
    coords.into_iter().filter(|(x, y)| x.is_finite() && y.is_finite()).map(|(x, y)| project(x, y)).collect()
}

fn lines_from(lines: Vec<Vec<(f64, f64)>>) -> Vec<Vec<P>> {
    lines
        .into_iter()
        .flat_map(split_antimeridian)
        .map(|part| {
            let mut pts = points_from(part);
            pts.dedup();
            pts
        })
        .filter(|p| p.len() >= 2)
        .collect()
}

fn ring_from(ring: Vec<(f64, f64)>) -> Option<Vec<P>> {
    let mut pts = points_from(ring);
    pts.dedup();
    if pts.len() > 1 && pts.first() == pts.last() {
        pts.pop();
    }
    (pts.len() >= 3).then_some(pts)
}

fn polygons_from(polys: Vec<Vec<Vec<(f64, f64)>>>) -> Vec<Vec<Vec<P>>> {
    polys
        .into_iter()
        .filter_map(|rings| {
            let mut rings = rings.into_iter();
            let outer = ring_from(rings.next()?)?;
            let mut out = vec![outer];
            out.extend(rings.filter_map(ring_from));
            Some(out)
        })
        .collect()
}

// ── GeoJSON ─────────────────────────────────────────────────────────────────

fn json_pos(v: &Json) -> Option<(f64, f64)> {
    let a = v.as_array()?;
    Some((a.first()?.as_f64()?, a.get(1)?.as_f64()?))
}

fn json_line(v: &Json) -> Vec<(f64, f64)> {
    v.as_array().map(|a| a.iter().filter_map(json_pos).collect()).unwrap_or_default()
}

fn json_poly(v: &Json) -> Vec<Vec<(f64, f64)>> {
    v.as_array().map(|a| a.iter().map(json_line).collect()).unwrap_or_default()
}

/// Geometries of one GeoJSON geometry object (collections become several).
pub fn geojson_geometries(g: &Json) -> Vec<Geometry> {
    let coords = g.get("coordinates").unwrap_or(&Json::Null);
    let geometry = match g.get("type").and_then(Json::as_str) {
        Some("Point") => json_pos(coords).map(|p| Geometry::Points(points_from([p]))),
        Some("MultiPoint") => Some(Geometry::Points(points_from(json_line(coords)))),
        Some("LineString") => Some(Geometry::Lines(lines_from(vec![json_line(coords)]))),
        Some("MultiLineString") => Some(Geometry::Lines(lines_from(json_poly(coords)))),
        Some("Polygon") => Some(Geometry::Polygons(polygons_from(vec![json_poly(coords)]))),
        Some("MultiPolygon") => {
            Some(Geometry::Polygons(polygons_from(coords.as_array().map(|a| a.iter().map(json_poly).collect()).unwrap_or_default())))
        }
        Some("GeometryCollection") => {
            return g
                .get("geometries")
                .and_then(Json::as_array)
                .map(|gs| gs.iter().flat_map(geojson_geometries).collect())
                .unwrap_or_default()
        }
        _ => None,
    };
    geometry.into_iter().filter(|g| !is_empty(g)).collect()
}

pub fn is_empty(g: &Geometry) -> bool {
    match g {
        Geometry::Points(p) => p.is_empty(),
        Geometry::Lines(l) => l.is_empty(),
        Geometry::Polygons(p) => p.is_empty(),
    }
}

fn json_value(v: &Json) -> Option<Value> {
    match v {
        Json::Null => None,
        Json::Bool(b) => Some(Value::Bool(*b)),
        Json::Number(n) => Some(n.as_i64().map(Value::Int).unwrap_or_else(|| Value::Double(n.as_f64().unwrap_or(0.0)))),
        Json::String(s) => Some(Value::String(s.clone())),
        other => Some(Value::String(other.to_string())),
    }
}

fn zoom_prop(v: Option<&Json>) -> Option<u8> {
    v.and_then(Json::as_f64).filter(|z| z.is_finite() && *z >= 0.0).map(|z| z.min(24.0) as u8)
}

fn geojson_feature(f: &Json, opts: &InputOptions, keys: &mut Interner, sink: &mut dyn FnMut(SourceFeature), stats: &mut ReadStats) {
    let Some(geometry) = f.get("geometry").filter(|g| !g.is_null()) else {
        stats.skipped += 1;
        return;
    };
    let props = f.get("properties").and_then(Json::as_object);
    let tippecanoe = f.get("tippecanoe");
    let minzoom = zoom_prop(props.and_then(|p| p.get("_minzoom"))).or_else(|| zoom_prop(tippecanoe.and_then(|t| t.get("minzoom"))));
    let maxzoom = zoom_prop(props.and_then(|p| p.get("_maxzoom"))).or_else(|| zoom_prop(tippecanoe.and_then(|t| t.get("maxzoom"))));
    let properties: Vec<(Arc<str>, Value)> = props
        .map(|p| p.iter().filter(|(k, _)| keep_attr(opts, k)).filter_map(|(k, v)| Some((keys.get(k), json_value(v)?))).collect())
        .unwrap_or_default();
    let geometries = geojson_geometries(geometry);
    if geometries.is_empty() {
        stats.skipped += 1;
    }
    for geometry in geometries {
        stats.features += 1;
        sink(SourceFeature { geometry, properties: properties.clone(), minzoom, maxzoom });
    }
}

fn read_geojson(path: &Path, opts: &InputOptions, sink: &mut dyn FnMut(SourceFeature)) -> Result<ReadStats> {
    let mut stats = ReadStats::default();
    let mut keys = Interner(HashMap::new());
    let file = std::fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut reader = BufReader::with_capacity(1 << 20, file);

    // Peek at the first non-blank line: a FeatureCollection is read whole,
    // anything else is treated as one feature per line.
    let mut first = String::new();
    while first.trim().is_empty() {
        first.clear();
        if reader.read_line(&mut first)? == 0 {
            return Ok(stats);
        }
    }
    let first_trim = first.trim_start_matches(['\u{1e}', '\u{feff}']).trim();
    let whole_line: Option<Json> = serde_json::from_str(first_trim).ok();
    let is_collection = whole_line.as_ref().map(|v| v.get("type").and_then(Json::as_str) == Some("FeatureCollection")).unwrap_or(true);

    if let (false, Some(first_feature)) = (is_collection, whole_line) {
        geojson_feature(&first_feature, opts, &mut keys, sink, &mut stats);
        for (n, line) in reader.lines().enumerate() {
            let line = line?;
            let line = line.trim_start_matches('\u{1e}').trim();
            if line.is_empty() {
                continue;
            }
            match serde_json::from_str::<Json>(line) {
                Ok(f) => geojson_feature(&f, opts, &mut keys, sink, &mut stats),
                Err(e) => bail!("{}: line {}: {e}", path.display(), n + 2),
            }
        }
        return Ok(stats);
    }

    let mut rest = Vec::new();
    reader.read_to_end(&mut rest)?;
    let mut text = first.into_bytes();
    text.extend(rest);
    let text = String::from_utf8(text).context("GeoJSON must be UTF-8")?;
    let data: Json = serde_json::from_str(text.trim_start_matches('\u{feff}')).with_context(|| format!("parsing {}", path.display()))?;
    match data.get("type").and_then(Json::as_str) {
        Some("FeatureCollection") => {
            for f in data.get("features").and_then(Json::as_array).map(Vec::as_slice).unwrap_or_default() {
                geojson_feature(f, opts, &mut keys, sink, &mut stats);
            }
        }
        Some("Feature") => geojson_feature(&data, opts, &mut keys, sink, &mut stats),
        _ => bail!("{} is not a GeoJSON Feature or FeatureCollection", path.display()),
    }
    Ok(stats)
}

// ── GeoPackage / WKB ────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Debug)]
enum Crs {
    LonLat,
    WebMercator,
}

struct Wkb<'a> {
    data: &'a [u8],
    pos: usize,
    crs: Crs,
}

#[derive(Default)]
struct Parts {
    points: Vec<(f64, f64)>,
    lines: Vec<Vec<(f64, f64)>>,
    polygons: Vec<Vec<Vec<(f64, f64)>>>,
}

impl<'a> Wkb<'a> {
    fn take<const N: usize>(&mut self) -> Result<[u8; N]> {
        let end = self.pos + N;
        let bytes = self.data.get(self.pos..end).ok_or_else(|| anyhow::anyhow!("truncated WKB"))?;
        self.pos = end;
        Ok(bytes.try_into().expect("length checked"))
    }
    fn u32(&mut self, le: bool) -> Result<u32> {
        let b = self.take::<4>()?;
        Ok(if le { u32::from_le_bytes(b) } else { u32::from_be_bytes(b) })
    }
    fn f64(&mut self, le: bool) -> Result<f64> {
        let b = self.take::<8>()?;
        Ok(if le { f64::from_le_bytes(b) } else { f64::from_be_bytes(b) })
    }
    fn coord(&mut self, le: bool, dims: usize) -> Result<(f64, f64)> {
        let x = self.f64(le)?;
        let y = self.f64(le)?;
        for _ in 2..dims {
            self.f64(le)?;
        }
        Ok(match self.crs {
            Crs::LonLat => (x, y),
            Crs::WebMercator => mercator_meters_to_lon_lat(x, y),
        })
    }
    fn coords(&mut self, le: bool, dims: usize) -> Result<Vec<(f64, f64)>> {
        let n = self.u32(le)? as usize;
        if n > self.data.len() / 16 + 1 {
            bail!("corrupt WKB point count");
        }
        (0..n).map(|_| self.coord(le, dims)).collect()
    }

    fn geometry(&mut self, out: &mut Parts, depth: usize) -> Result<()> {
        if depth > 16 {
            bail!("WKB nesting too deep");
        }
        let le = self.take::<1>()?[0] == 1;
        let raw = self.u32(le)?;
        // ISO (1000/2000/3000 offsets) and EWKB (high-bit flags) dimension encodings.
        let mut dims = 2;
        if raw & 0x8000_0000 != 0 {
            dims += 1;
        }
        if raw & 0x4000_0000 != 0 {
            dims += 1;
        }
        if raw & 0x2000_0000 != 0 {
            self.u32(le)?; // embedded SRID
        }
        let code = raw & 0x0FFF_FFFF;
        let base = code % 1000;
        dims += match code / 1000 {
            1 | 2 => 1,
            3 => 2,
            _ => 0,
        };
        match base {
            1 => {
                let c = self.coord(le, dims)?;
                if !(c.0.is_nan() && c.1.is_nan()) {
                    out.points.push(c);
                }
            }
            2 => out.lines.push(self.coords(le, dims)?),
            3 => {
                let rings = self.u32(le)? as usize;
                let mut poly = Vec::with_capacity(rings.min(1024));
                for _ in 0..rings {
                    poly.push(self.coords(le, dims)?);
                }
                out.polygons.push(poly);
            }
            4..=7 => {
                let n = self.u32(le)?;
                for _ in 0..n {
                    self.geometry(out, depth + 1)?;
                }
            }
            other => bail!("unsupported WKB geometry type {other}"),
        }
        Ok(())
    }
}

/// Parse a GeoPackage geometry blob into lon/lat parts.
fn gpkg_geometries(blob: &[u8], crs: Crs) -> Result<Vec<Geometry>> {
    if blob.len() < 8 || &blob[0..2] != b"GP" {
        bail!("not a GeoPackage geometry");
    }
    let flags = blob[3];
    if flags & 0b1_0000 != 0 {
        return Ok(Vec::new()); // empty geometry
    }
    if flags & 0b10_0000 != 0 {
        bail!("extended GeoPackage geometry types are not supported");
    }
    let envelope = match (flags >> 1) & 0b111 {
        0 => 0,
        1 => 32,
        2 | 3 => 48,
        4 => 64,
        _ => bail!("invalid GeoPackage envelope"),
    };
    let mut wkb = Wkb { data: blob, pos: 8 + envelope, crs };
    let mut parts = Parts::default();
    wkb.geometry(&mut parts, 0)?;
    let mut out = Vec::new();
    if !parts.points.is_empty() {
        out.push(Geometry::Points(points_from(parts.points)));
    }
    let lines = lines_from(parts.lines);
    if !lines.is_empty() {
        out.push(Geometry::Lines(lines));
    }
    let polygons = polygons_from(parts.polygons);
    if !polygons.is_empty() {
        out.push(Geometry::Polygons(polygons));
    }
    Ok(out)
}

/// Feature tables in a GeoPackage.
pub fn gpkg_tables(path: &Path) -> Result<Vec<(String, String, i64)>> {
    let conn =
        Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).with_context(|| format!("opening {}", path.display()))?;
    let mut stmt = conn.prepare(
        "SELECT c.table_name, g.geometry_type_name, c.srs_id FROM gpkg_contents c
         JOIN gpkg_geometry_columns g ON g.table_name = c.table_name WHERE c.data_type = 'features' ORDER BY c.table_name",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn crs_for(conn: &Connection, srs_id: i64) -> Result<Crs> {
    let (org, code, definition): (String, i64, String) = conn
        .query_row("SELECT organization, organization_coordsys_id, definition FROM gpkg_spatial_ref_sys WHERE srs_id = ?1", [srs_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .unwrap_or_else(|_| (String::new(), srs_id, String::new()));
    let def = definition.to_ascii_uppercase();
    match (org.to_ascii_uppercase().as_str(), code) {
        (_, 4326) => Ok(Crs::LonLat),
        (_, 3857) | (_, 900913) => Ok(Crs::WebMercator),
        _ if def.starts_with("GEOGCS") && def.contains("WGS") => Ok(Crs::LonLat),
        _ if srs_id == 4326 => Ok(Crs::LonLat),
        _ => bail!("unsupported coordinate system (srs_id {srs_id}, {org}:{code}); reproject to EPSG:4326 first"),
    }
}

fn read_gpkg(path: &Path, opts: &InputOptions, sink: &mut dyn FnMut(SourceFeature)) -> Result<ReadStats> {
    let tables = gpkg_tables(path)?;
    let table = match &opts.gpkg_table {
        Some(t) => tables.iter().find(|(name, ..)| name.eq_ignore_ascii_case(t)).map(|(name, ..)| name.clone()).ok_or_else(|| {
            anyhow::anyhow!("table '{t}' not found; available: {}", tables.iter().map(|t| t.0.as_str()).collect::<Vec<_>>().join(", "))
        })?,
        None if tables.len() == 1 => tables[0].0.clone(),
        None => bail!(
            "{} has {} feature tables; choose one with --gpkg-table: {}",
            path.display(),
            tables.len(),
            tables.iter().map(|t| t.0.as_str()).collect::<Vec<_>>().join(", ")
        ),
    };
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let (geom_col, srs_id): (String, i64) =
        conn.query_row("SELECT column_name, srs_id FROM gpkg_geometry_columns WHERE table_name = ?1", [&table], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?;
    let crs = crs_for(&conn, srs_id)?;

    let mut columns = Vec::new();
    {
        let mut stmt = conn.prepare(&format!("PRAGMA table_info(\"{}\")", table.replace('"', "\"\"")))?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(1)?, r.get::<_, i64>(5)?)))?;
        for (name, pk) in rows.flatten() {
            if name != geom_col && pk == 0 && keep_attr(opts, &name) {
                columns.push(name);
            }
        }
    }
    let quote = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
    let mut sql = format!("SELECT {}", quote(&geom_col));
    for c in &columns {
        sql.push_str(", ");
        sql.push_str(&quote(c));
    }
    sql.push_str(&format!(" FROM {}", quote(&table)));
    if let Some(w) = &opts.where_clause {
        sql.push_str(&format!(" WHERE {w}"));
    }

    let mut keys = Interner(HashMap::new());
    let key_names: Vec<Arc<str>> = columns.iter().map(|c| keys.get(c)).collect();
    let mut stats = ReadStats::default();
    let mut stmt = conn.prepare(&sql).with_context(|| format!("querying {table}"))?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let Ok(ValueRef::Blob(blob)) = row.get_ref(0) else {
            stats.skipped += 1;
            continue;
        };
        let geometries = match gpkg_geometries(blob, crs) {
            Ok(g) if !g.is_empty() => g,
            _ => {
                stats.skipped += 1;
                continue;
            }
        };
        let mut properties = Vec::with_capacity(columns.len());
        for (i, key) in key_names.iter().enumerate() {
            let value = match row.get_ref(i + 1)? {
                ValueRef::Integer(v) => Value::Int(v),
                ValueRef::Real(v) => Value::Double(v),
                ValueRef::Text(t) => {
                    let s = String::from_utf8_lossy(t).trim().to_string();
                    if s.is_empty() {
                        continue;
                    }
                    Value::String(s)
                }
                ValueRef::Null | ValueRef::Blob(_) => continue,
            };
            properties.push((key.clone(), value));
        }
        for geometry in geometries {
            stats.features += 1;
            sink(SourceFeature { geometry, properties: properties.clone(), minzoom: None, maxzoom: None });
        }
    }
    Ok(stats)
}

/// Read any supported file, calling `sink` for every feature.
pub fn read(path: &Path, opts: &InputOptions, sink: &mut dyn FnMut(SourceFeature)) -> Result<ReadStats> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "gpkg" => read_gpkg(path, opts, sink),
        "geojson" | "json" | "geojsonl" | "geojsons" | "geojsonseq" | "ndjson" | "jsonl" => read_geojson(path, opts, sink),
        _ => bail!("{}: unsupported input (use .geojson, .geojsonl or .gpkg)", path.display()),
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    fn collect(path: &Path, opts: &InputOptions) -> Vec<SourceFeature> {
        let mut out = Vec::new();
        read(path, opts, &mut |f| out.push(f)).unwrap();
        out
    }

    pub fn temp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("worldmap-tilegen-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    #[test]
    fn reads_collections_and_sequences_with_attribute_filters() {
        let collection = temp("c.geojson");
        std::fs::write(
            &collection,
            r#"{"type":"FeatureCollection","features":[
                {"type":"Feature","geometry":{"type":"LineString","coordinates":[[0,0],[1,1],[1,1]]},"properties":{"voltage":380000,"name":"A","_minzoom":5,"skip":null}},
                {"type":"Feature","geometry":null,"properties":{}},
                {"type":"Feature","geometry":{"type":"GeometryCollection","geometries":[{"type":"Point","coordinates":[5,5]},{"type":"Polygon","coordinates":[[[0,0],[1,0],[1,1],[0,0]]]}]},"properties":{"k":true}}
            ]}"#,
        )
        .unwrap();
        let features = collect(&collection, &InputOptions::default());
        assert_eq!(features.len(), 3);
        assert_eq!(features[0].minzoom, Some(5));
        assert!(matches!(&features[0].geometry, Geometry::Lines(l) if l[0].len() == 2), "duplicate vertices removed");
        assert_eq!(features[0].properties.len(), 2, "null and _-prefixed attributes are dropped");
        assert!(matches!(&features[2].geometry, Geometry::Polygons(p) if p[0][0].len() == 3), "closing point removed");

        let seq = temp("s.geojsonl");
        std::fs::write(
            &seq,
            "\u{1e}{\"type\":\"Feature\",\"geometry\":{\"type\":\"Point\",\"coordinates\":[1,2]},\"properties\":{\"a\":1,\"b\":2}}\n\n{\"type\":\"Feature\",\"geometry\":{\"type\":\"MultiLineString\",\"coordinates\":[[[179,0],[-179,0.1],[-178,0]]]},\"properties\":{\"a\":1}}\n",
        )
        .unwrap();
        let opts = InputOptions { include: Some(["a".to_string()].into()), ..Default::default() };
        let features = collect(&seq, &opts);
        assert_eq!(features.len(), 2);
        assert_eq!(features[0].properties.len(), 1);
        assert!(matches!(&features[1].geometry, Geometry::Lines(l) if l.len() == 1 && l[0].len() == 2), "antimeridian jump split");
    }

    /// Build a GeoPackage by hand, as GDAL would write it.
    pub fn write_gpkg(path: &Path, srs_id: i64) {
        let _ = std::fs::remove_file(path);
        let conn = Connection::open(path).unwrap();
        conn.execute_batch(&format!(
            "CREATE TABLE gpkg_spatial_ref_sys (srs_name TEXT, srs_id INTEGER PRIMARY KEY, organization TEXT, organization_coordsys_id INTEGER, definition TEXT, description TEXT);
             INSERT INTO gpkg_spatial_ref_sys VALUES ('WGS 84', 4326, 'EPSG', 4326, 'GEOGCS[\"WGS 84\"]', NULL), ('Pseudo-Mercator', 3857, 'EPSG', 3857, 'PROJCS[\"WGS 84 / Pseudo-Mercator\"]', NULL);
             CREATE TABLE gpkg_contents (table_name TEXT PRIMARY KEY, data_type TEXT, identifier TEXT, srs_id INTEGER);
             CREATE TABLE gpkg_geometry_columns (table_name TEXT, column_name TEXT, geometry_type_name TEXT, srs_id INTEGER, z INTEGER, m INTEGER);
             INSERT INTO gpkg_contents VALUES ('grid', 'features', 'grid', {srs_id});
             INSERT INTO gpkg_geometry_columns VALUES ('grid', 'geom', 'MULTILINESTRING', {srs_id}, 0, 0);
             CREATE TABLE grid (fid INTEGER PRIMARY KEY, geom BLOB, source TEXT, kv REAL);"
        ))
        .unwrap();
        let scale: f64 = if srs_id == 3857 { 111_319.490_793_273_58 } else { 1.0 };
        // GP header (little endian, envelope type 1) + ISO WKB MultiLineString Z.
        let mut blob = vec![b'G', b'P', 0, 0b0000_0011];
        blob.extend_from_slice(&(srs_id as i32).to_le_bytes());
        for v in [10.0f64, 12.0, 50.0, 51.0] {
            blob.extend_from_slice(&(v * scale).to_le_bytes());
        }
        blob.push(1);
        blob.extend_from_slice(&1005u32.to_le_bytes());
        blob.extend_from_slice(&1u32.to_le_bytes());
        blob.push(1);
        blob.extend_from_slice(&1002u32.to_le_bytes());
        blob.extend_from_slice(&3u32.to_le_bytes());
        for (x, y) in [(10.0f64, 50.0f64), (11.0, 50.5), (12.0, 51.0)] {
            blob.extend_from_slice(&(x * scale).to_le_bytes());
            blob.extend_from_slice(&(y * if srs_id == 3857 { 0.0f64 } else { 1.0 }).to_le_bytes());
            blob.extend_from_slice(&0.0f64.to_le_bytes());
        }
        conn.execute("INSERT INTO grid (geom, source, kv) VALUES (?1, 'test', 380.0)", [&blob]).unwrap();
        conn.execute("INSERT INTO grid (geom, source, kv) VALUES (NULL, 'no geometry', 1.0)", []).unwrap();
    }

    #[test]
    fn reads_geopackage_multilinestring_z() {
        let path = temp("grid.gpkg");
        write_gpkg(&path, 4326);
        let mut features = Vec::new();
        let result = read(&path, &InputOptions::default(), &mut |f| features.push(f)).unwrap();
        assert_eq!((result.features, result.skipped), (1, 1));
        let Geometry::Lines(lines) = &features[0].geometry else { panic!() };
        assert_eq!(lines[0].len(), 3);
        assert_eq!(lines[0][0], project(10.0, 50.0));
        assert!(features[0].properties.iter().any(|(k, v)| &**k == "kv" && *v == Value::Double(380.0)));
        assert!(!features[0].properties.iter().any(|(k, _)| &**k == "fid"), "primary key is not an attribute");

        let filtered = {
            let mut n = 0;
            read(&path, &InputOptions { where_clause: Some("kv > 100".into()), ..Default::default() }, &mut |_| n += 1).unwrap();
            n
        };
        assert_eq!(filtered, 1);
        assert_eq!(gpkg_tables(&path).unwrap()[0].0, "grid");
    }

    #[test]
    fn reads_web_mercator_geopackages() {
        let path = temp("merc.gpkg");
        write_gpkg(&path, 3857);
        let features = collect(&path, &InputOptions::default());
        let Geometry::Lines(lines) = &features[0].geometry else { panic!() };
        let expected = project(10.0, 0.0);
        assert!((lines[0][0][0] - expected[0]).abs() < 1e-9 && (lines[0][0][1] - expected[1]).abs() < 1e-9);
    }
}
