//! Vector tile builder: GeoJSON / GeoJSONSeq / GeoPackage → MBTiles.
//!
//! For every zoom level each feature is simplified to the tile resolution,
//! clipped to the tiles it touches (plus a small buffer so line joins render
//! seamlessly) and encoded as Mapbox Vector Tiles. Vertices are snapped to the
//! tile grid, so pieces too small to see vanish without opening gaps, and an
//! oversized tile sheds its smallest features first: networks stay connected
//! at every zoom.

pub mod cli;
pub mod geometry;
pub mod input;
pub mod mbtiles;
pub mod mvt;

use anyhow::{bail, Result};
use rayon::prelude::*;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use geometry::{clip_line, clip_ring, length, signed_area, simplify, unproject, Rect, P};
use input::{Geometry, InputOptions};
#[cfg(test)]
use mvt::Value;
use mvt::{GeomType, Properties, TileFeature, EXTENT};

#[derive(Clone)]
pub struct BuildOptions {
    pub output: PathBuf,
    pub inputs: Vec<PathBuf>,
    pub input_options: InputOptions,
    pub layer: String,
    pub name: String,
    pub description: Option<String>,
    pub attribution: Option<String>,
    pub minzoom: u8,
    pub maxzoom: u8,
    /// Douglas–Peucker tolerance in tile units (of 4096) below the max zoom.
    pub simplify: f64,
    /// Clip buffer in tile units.
    pub buffer: u32,
    /// Lines shorter than this many 256-px pixels are left out below max zoom
    /// (0 keeps everything; useful for sparse, independent features).
    pub min_length_px: f64,
    /// Compressed size limit per tile.
    pub max_tile_bytes: usize,
    /// Merge features with identical attributes within a tile.
    pub coalesce: bool,
}

impl BuildOptions {
    pub fn new(output: PathBuf, inputs: Vec<PathBuf>, layer: &str) -> Self {
        Self {
            name: output.file_stem().and_then(|s| s.to_str()).unwrap_or("tiles").to_string(),
            output,
            inputs,
            input_options: InputOptions::default(),
            layer: layer.to_string(),
            description: None,
            attribution: None,
            minzoom: 0,
            maxzoom: 12,
            simplify: 1.0,
            buffer: 64,
            // Networks are made of many short segments: dropping short ones would
            // break them apart. Quantisation already removes invisible pieces.
            min_length_px: 0.0,
            max_tile_bytes: 500 * 1024,
            coalesce: true,
        }
    }
}

struct Feature {
    geometry: Geometry,
    properties: Properties,
    minzoom: u8,
    maxzoom: u8,
    bbox: Rect,
    priority: f64,
}

#[derive(Debug, Default, Clone)]
pub struct ZoomReport {
    pub zoom: u8,
    pub tiles: usize,
    pub bytes: usize,
    pub largest: usize,
    pub dropped_small: usize,
    pub dropped_for_size: usize,
}

#[derive(Debug, Default, Clone)]
pub struct BuildReport {
    pub features: u64,
    pub skipped: u64,
    pub zooms: Vec<ZoomReport>,
}

impl BuildReport {
    pub fn tiles(&self) -> usize {
        self.zooms.iter().map(|z| z.tiles).sum()
    }
    pub fn bytes(&self) -> usize {
        self.zooms.iter().map(|z| z.bytes).sum()
    }
}

pub(crate) fn bbox_of(g: &Geometry) -> Rect {
    let mut r = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
    let mut add = |p: &P| {
        r[0] = r[0].min(p[0]);
        r[1] = r[1].min(p[1]);
        r[2] = r[2].max(p[0]);
        r[3] = r[3].max(p[1]);
    };
    match g {
        Geometry::Points(pts) => pts.iter().for_each(&mut add),
        Geometry::Lines(lines) => lines.iter().flatten().for_each(&mut add),
        Geometry::Polygons(polys) => polys.iter().flat_map(|p| p.first()).flatten().for_each(&mut add),
    }
    r
}

fn pseudo_random(i: usize) -> f64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    i.hash(&mut h);
    (h.finish() >> 11) as f64 / (1u64 << 53) as f64
}

pub(crate) fn simplify_geometry(g: &Geometry, tolerance: f64, min_length: f64) -> Option<Geometry> {
    match g {
        Geometry::Points(p) => Some(Geometry::Points(p.clone())),
        Geometry::Lines(lines) => {
            let out: Vec<Vec<P>> = lines.iter().map(|l| simplify(l, tolerance)).filter(|l| l.len() >= 2).collect();
            let total: f64 = out.iter().map(|l| length(l)).sum();
            (total >= min_length && !out.is_empty()).then_some(Geometry::Lines(out))
        }
        Geometry::Polygons(polys) => {
            let min_area = min_length * min_length;
            let out: Vec<Vec<Vec<P>>> = polys
                .iter()
                .filter_map(|rings| {
                    let rings: Vec<Vec<P>> =
                        rings.iter().map(|r| simplify(r, tolerance)).filter(|r| r.len() >= 3 && signed_area(r).abs() >= min_area).collect();
                    (!rings.is_empty()).then_some(rings)
                })
                .collect();
            (!out.is_empty()).then_some(Geometry::Polygons(out))
        }
    }
}

/// Tiles (x, y) a geometry touches at zoom `n` tiles per axis.
fn covered_tiles(g: &Geometry, bbox: &Rect, n: f64, buffer: f64) -> Vec<(u32, u32)> {
    let max = n as i64 - 1;
    let range = |lo: f64, hi: f64, b: f64| ((lo * n - b).floor().max(0.0) as i64, ((hi * n + b).floor() as i64).min(max));
    match g {
        Geometry::Points(pts) => {
            let mut set: HashSet<(u32, u32)> = HashSet::new();
            for p in pts {
                let x = ((p[0] * n).floor() as i64).clamp(0, max) as u32;
                let y = ((p[1] * n).floor() as i64).clamp(0, max) as u32;
                set.insert((x, y));
            }
            set.into_iter().collect()
        }
        Geometry::Lines(lines) => {
            let (x0, x1) = range(bbox[0], bbox[2], buffer);
            let (y0, y1) = range(bbox[1], bbox[3], buffer);
            if (x1 - x0 + 1) * (y1 - y0 + 1) <= 4 {
                return (x0..=x1).flat_map(|x| (y0..=y1).map(move |y| (x as u32, y as u32))).collect();
            }
            // Walk segments so a diagonal line does not visit its whole bbox.
            let mut set: HashSet<(u32, u32)> = HashSet::new();
            for line in lines {
                for w in line.windows(2) {
                    let (sx0, sx1) = range(w[0][0].min(w[1][0]), w[0][0].max(w[1][0]), buffer);
                    let (sy0, sy1) = range(w[0][1].min(w[1][1]), w[0][1].max(w[1][1]), buffer);
                    for x in sx0..=sx1 {
                        for y in sy0..=sy1 {
                            set.insert((x as u32, y as u32));
                        }
                    }
                }
            }
            set.into_iter().collect()
        }
        Geometry::Polygons(_) => {
            let (x0, x1) = range(bbox[0], bbox[2], buffer);
            let (y0, y1) = range(bbox[1], bbox[3], buffer);
            (x0..=x1).flat_map(|x| (y0..=y1).map(move |y| (x as u32, y as u32))).collect()
        }
    }
}

fn to_tile(p: P, n: f64, tx: u32, ty: u32) -> [i32; 2] {
    [((p[0] * n - tx as f64) * EXTENT as f64).round() as i32, ((p[1] * n - ty as f64) * EXTENT as f64).round() as i32]
}

fn quantize(points: &[P], n: f64, tx: u32, ty: u32) -> Vec<[i32; 2]> {
    let mut out: Vec<[i32; 2]> = points.iter().map(|p| to_tile(*p, n, tx, ty)).collect();
    out.dedup();
    out
}

fn ring_area(ring: &[[i32; 2]]) -> i64 {
    let n = ring.len();
    let mut sum = 0i64;
    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + 1) % n]);
        sum += a[0] as i64 * b[1] as i64 - b[0] as i64 * a[1] as i64;
    }
    sum
}

/// Clip a simplified geometry to one tile and convert it to tile coordinates.
pub(crate) fn clip_to_tile(g: &Geometry, bbox: &Rect, n: f64, tx: u32, ty: u32, buffer: f64) -> Option<(GeomType, Vec<Vec<[i32; 2]>>)> {
    let b = buffer / n;
    let rect = [tx as f64 / n - b, ty as f64 / n - b, (tx + 1) as f64 / n + b, (ty + 1) as f64 / n + b];
    let contained = bbox[0] >= rect[0] && bbox[1] >= rect[1] && bbox[2] <= rect[2] && bbox[3] <= rect[3];
    match g {
        Geometry::Points(pts) => {
            // Half-open tile bounds: a point belongs to exactly one tile.
            let (x0, y0, x1, y1) = (tx as f64 / n, ty as f64 / n, (tx + 1) as f64 / n, (ty + 1) as f64 / n);
            let last = n - 1.0;
            let parts: Vec<Vec<[i32; 2]>> = pts
                .iter()
                .filter(|p| p[0] >= x0 && (p[0] < x1 || tx as f64 == last) && p[1] >= y0 && (p[1] < y1 || ty as f64 == last))
                .map(|p| vec![to_tile(*p, n, tx, ty)])
                .collect();
            (!parts.is_empty()).then_some((GeomType::Point, parts))
        }
        Geometry::Lines(lines) => {
            let mut parts = Vec::new();
            for line in lines {
                let pieces = if contained { vec![line.clone()] } else { clip_line(line, &rect) };
                for piece in pieces {
                    let q = quantize(&piece, n, tx, ty);
                    if q.len() >= 2 {
                        parts.push(q);
                    }
                }
            }
            (!parts.is_empty()).then_some((GeomType::LineString, parts))
        }
        Geometry::Polygons(polys) => {
            let mut rings_out = Vec::new();
            for rings in polys {
                let mut first = true;
                for ring in rings {
                    let clipped = if contained { ring.clone() } else { clip_ring(ring, &rect) };
                    let mut q = quantize(&clipped, n, tx, ty);
                    while q.len() > 1 && q.first() == q.last() {
                        q.pop();
                    }
                    let area = ring_area(&q);
                    if q.len() < 3 || area == 0 {
                        if first {
                            break; // exterior vanished: drop the polygon and its holes
                        }
                        continue;
                    }
                    // Exterior rings positive (clockwise, y down), holes negative.
                    if (first && area < 0) || (!first && area > 0) {
                        q.reverse();
                    }
                    rings_out.push(q);
                    first = false;
                }
            }
            (!rings_out.is_empty()).then_some((GeomType::Polygon, rings_out))
        }
    }
}

type GroupKey = (GeomType, Vec<(Arc<str>, String)>);

/// Join line parts that meet end-to-start (or end-to-end) into longer lines.
pub fn merge_lines(parts: Vec<Vec<[i32; 2]>>) -> Vec<Vec<[i32; 2]>> {
    if parts.len() < 2 {
        return parts;
    }
    let mut ends: HashMap<[i32; 2], Vec<usize>> = HashMap::new();
    for (i, p) in parts.iter().enumerate() {
        ends.entry(p[0]).or_default().push(i);
        ends.entry(p[p.len() - 1]).or_default().push(i);
    }
    let mut used = vec![false; parts.len()];
    let mut out = Vec::new();
    for start in 0..parts.len() {
        if used[start] {
            continue;
        }
        used[start] = true;
        let mut line = parts[start].clone();
        // Extend forwards, then backwards from the reversed line.
        for _ in 0..2 {
            loop {
                let tail = line[line.len() - 1];
                let Some(next) = ends.get(&tail).and_then(|c| c.iter().copied().find(|&j| !used[j])) else { break };
                used[next] = true;
                let mut piece = parts[next].clone();
                if piece[0] != tail {
                    piece.reverse();
                }
                line.extend_from_slice(&piece[1..]);
            }
            line.reverse();
        }
        out.push(line);
    }
    out
}

fn coalesce(features: Vec<TileFeature>) -> Vec<TileFeature> {
    let mut groups: HashMap<GroupKey, usize> = HashMap::new();
    let mut merged: Vec<TileFeature> = Vec::new();
    for f in features {
        let key = (f.geom_type, f.properties.iter().map(|(k, v)| (k.clone(), format!("{v:?}"))).collect::<Vec<_>>());
        match groups.get(&key) {
            Some(&i) => {
                merged[i].parts.extend(f.parts);
                merged[i].priority += f.priority;
            }
            None => {
                groups.insert(key, merged.len());
                merged.push(f);
            }
        }
    }
    for f in &mut merged {
        if f.geom_type == GeomType::LineString {
            f.parts = merge_lines(std::mem::take(&mut f.parts));
        }
    }
    merged
}

fn gzip(data: &[u8]) -> Vec<u8> {
    let mut e = flate2::write::GzEncoder::new(Vec::with_capacity(data.len() / 3), flate2::Compression::new(6));
    e.write_all(data).expect("in-memory write");
    e.finish().expect("in-memory write")
}

/// Encode a tile within the size limit. Returns (gzipped tile, dropped features).
pub(crate) fn encode_limited(layer: &str, mut features: Vec<TileFeature>, max_bytes: usize, merge: bool) -> (Vec<u8>, usize) {
    let total = features.len();
    features.sort_by(|a, b| b.priority.total_cmp(&a.priority));
    let mut keep = total;
    loop {
        let subset: Vec<TileFeature> = features[..keep].to_vec();
        let subset = if merge { coalesce(subset) } else { subset };
        let data = gzip(&mvt::encode_tile(&[(layer, &subset)]));
        if data.len() <= max_bytes || keep <= 1 {
            return (data, total - keep);
        }
        let ratio = max_bytes as f64 / data.len() as f64;
        keep = ((keep as f64 * ratio * 0.9) as usize).clamp(1, keep - 1);
    }
}

fn field_types(features: &[Feature]) -> BTreeMap<String, &'static str> {
    let mut out: BTreeMap<String, &'static str> = BTreeMap::new();
    for f in features {
        for (k, v) in f.properties.iter() {
            let t = v.type_name();
            out.entry(k.to_string())
                .and_modify(|e| {
                    if *e != t {
                        *e = "Mixed";
                    }
                })
                .or_insert(t);
        }
    }
    out
}

pub fn build(opts: &BuildOptions) -> Result<BuildReport> {
    if opts.minzoom > opts.maxzoom || opts.maxzoom > 18 {
        bail!("zoom range must satisfy 0 <= minzoom <= maxzoom <= 18");
    }
    if opts.layer.is_empty() || !opts.layer.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        bail!("layer name must be alphanumeric (with - or _)");
    }
    let started = Instant::now();
    let mut report = BuildReport::default();
    let mut features: Vec<Feature> = Vec::new();
    for path in &opts.inputs {
        tracing::info!("Reading {}", path.display());
        let mut index = features.len();
        let stats = input::read(path, &opts.input_options, &mut |f| {
            let bbox = bbox_of(&f.geometry);
            let priority = match &f.geometry {
                Geometry::Points(_) => pseudo_random(index),
                Geometry::Lines(lines) => lines.iter().map(|l| length(l)).sum(),
                Geometry::Polygons(polys) => polys.iter().filter_map(|p| p.first()).map(|r| signed_area(r).abs().sqrt() * 4.0).sum(),
            };
            index += 1;
            features.push(Feature {
                geometry: f.geometry,
                properties: Arc::new(f.properties),
                minzoom: f.minzoom.unwrap_or(0).max(opts.minzoom),
                maxzoom: f.maxzoom.unwrap_or(u8::MAX).min(opts.maxzoom),
                bbox,
                priority,
            });
        })?;
        report.features += stats.features;
        report.skipped += stats.skipped;
        tracing::info!("  {} features ({} skipped)", stats.features, stats.skipped);
    }
    if features.is_empty() {
        bail!("no usable features in the input files");
    }

    let mut writer = mbtiles::Writer::create(&opts.output)?;
    let buffer = opts.buffer as f64 / EXTENT as f64;
    for z in opts.minzoom..=opts.maxzoom {
        let t = Instant::now();
        let n = (1u64 << z) as f64;
        let at_max = z == opts.maxzoom;
        let tolerance = if at_max { 0.0 } else { opts.simplify / (n * EXTENT as f64) };
        let min_length = if at_max { 0.0 } else { opts.min_length_px / (n * 256.0) };

        let (buckets, dropped_small) = features
            .par_iter()
            .filter(|f| f.minzoom <= z && z <= f.maxzoom)
            .fold(
                || (HashMap::<(u32, u32), Vec<TileFeature>>::new(), 0usize),
                |(mut acc, dropped), f| {
                    let Some(g) = simplify_geometry(&f.geometry, tolerance, min_length) else {
                        return (acc, dropped + 1);
                    };
                    let bbox = bbox_of(&g);
                    for (tx, ty) in covered_tiles(&g, &bbox, n, opts.buffer as f64 / EXTENT as f64) {
                        if let Some((geom_type, parts)) = clip_to_tile(&g, &bbox, n, tx, ty, buffer) {
                            acc.entry((tx, ty)).or_default().push(TileFeature {
                                geom_type,
                                parts,
                                properties: f.properties.clone(),
                                priority: f.priority,
                            });
                        }
                    }
                    (acc, dropped)
                },
            )
            .reduce(
                || (HashMap::new(), 0),
                |(a, da), (b, db)| {
                    let (mut big, small) = if a.len() >= b.len() { (a, b) } else { (b, a) };
                    for (k, v) in small {
                        big.entry(k).or_default().extend(v);
                    }
                    (big, da + db)
                },
            );

        let encoded: Vec<(u32, u32, Vec<u8>, usize)> = buckets
            .into_par_iter()
            .map(|((x, y), feats)| {
                let (data, dropped) = encode_limited(&opts.layer, feats, opts.max_tile_bytes, opts.coalesce);
                (x, y, data, dropped)
            })
            .filter(|(_, _, data, _)| !data.is_empty())
            .collect();

        let zr = ZoomReport {
            zoom: z,
            tiles: encoded.len(),
            bytes: encoded.iter().map(|t| t.2.len()).sum(),
            largest: encoded.iter().map(|t| t.2.len()).max().unwrap_or(0),
            dropped_small,
            dropped_for_size: encoded.iter().map(|t| t.3).sum(),
        };
        let tiles: Vec<(u32, u32, Vec<u8>)> = encoded.into_iter().map(|(x, y, d, _)| (x, y, d)).collect();
        writer.write_tiles(z, &tiles)?;
        tracing::info!(
            "  z{z:>2}: {:>7} tiles {:>9.1} KB (largest {:.0} KB){}{} in {:.1}s",
            zr.tiles,
            zr.bytes as f64 / 1024.0,
            zr.largest as f64 / 1024.0,
            if zr.dropped_small > 0 { format!(", {} sub-pixel features omitted", zr.dropped_small) } else { String::new() },
            if zr.dropped_for_size > 0 { format!(", {} dropped to fit size limit", zr.dropped_for_size) } else { String::new() },
            t.elapsed().as_secs_f64()
        );
        report.zooms.push(zr);
    }

    let mut bounds = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
    for f in &features {
        bounds[0] = bounds[0].min(f.bbox[0]);
        bounds[1] = bounds[1].min(f.bbox[1]);
        bounds[2] = bounds[2].max(f.bbox[2]);
        bounds[3] = bounds[3].max(f.bbox[3]);
    }
    let (west, north) = unproject([bounds[0], bounds[1]]);
    let (east, south) = unproject([bounds[2], bounds[3]]);
    let fields: serde_json::Map<String, serde_json::Value> = field_types(&features).into_iter().map(|(k, t)| (k, t.into())).collect();
    let json = serde_json::json!({
        "vector_layers": [{
            "id": opts.layer,
            "description": opts.description.clone().unwrap_or_default(),
            "minzoom": opts.minzoom,
            "maxzoom": opts.maxzoom,
            "fields": fields,
        }]
    });
    let mut meta = vec![
        ("name", opts.name.clone()),
        ("format", "pbf".to_string()),
        ("type", "overlay".to_string()),
        ("version", "2".to_string()),
        ("scheme", "xyz".to_string()),
        ("minzoom", opts.minzoom.to_string()),
        ("maxzoom", opts.maxzoom.to_string()),
        ("bounds", format!("{west:.6},{south:.6},{east:.6},{north:.6}")),
        ("center", format!("{:.6},{:.6},{}", (west + east) / 2.0, (south + north) / 2.0, opts.minzoom.max(2))),
        ("json", json.to_string()),
        ("generator", format!("worldmap-backend {} tiles", env!("CARGO_PKG_VERSION"))),
    ];
    if let Some(d) = &opts.description {
        meta.push(("description", d.clone()));
    }
    if let Some(a) = &opts.attribution {
        meta.push(("attribution", a.clone()));
    }
    writer.set_metadata(&meta)?;
    writer.finish()?;
    tracing::info!(
        "Wrote {} ({} tiles, {:.1} MB) in {:.1}s",
        opts.output.display(),
        report.tiles(),
        report.bytes() as f64 / 1_048_576.0,
        started.elapsed().as_secs_f64()
    );
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use std::io::Read;

    fn decode(data: &[u8]) -> Vec<mvt::DecodedLayer> {
        let mut raw = Vec::new();
        flate2::read::GzDecoder::new(data).read_to_end(&mut raw).unwrap();
        mvt::decode_tile(&raw).unwrap()
    }

    fn tile(path: &std::path::Path, z: u32, x: u32, y: u32) -> Option<Vec<u8>> {
        let conn = Connection::open(path).unwrap();
        conn.query_row(
            "SELECT tile_data FROM tiles WHERE zoom_level=?1 AND tile_column=?2 AND tile_row=?3",
            rusqlite::params![z, x, (1u32 << z) - 1 - y],
            |r| r.get(0),
        )
        .ok()
    }

    #[test]
    fn builds_a_seamless_line_network() {
        let src = input::tests::temp("net.geojsonl");
        // A 380 kV line from Berlin to Munich, crossing several tile borders at z6,
        // and a very short stub that must vanish at low zoom.
        std::fs::write(
            &src,
            concat!(
                r#"{"type":"Feature","geometry":{"type":"LineString","coordinates":[[13.4,52.5],[12.4,51.3],[11.6,48.1]]},"properties":{"voltage_kv":380,"name":"Nord–Süd"}}"#,
                "\n",
                r#"{"type":"Feature","geometry":{"type":"LineString","coordinates":[[10.0,50.0],[10.0001,50.0001]]},"properties":{"voltage_kv":110}}"#,
                "\n",
            ),
        )
        .unwrap();
        let out = input::tests::temp("net.mbtiles");
        let mut opts = BuildOptions::new(out.clone(), vec![src], "hvlines");
        opts.minzoom = 2;
        opts.maxzoom = 8;
        opts.min_length_px = 0.5;
        let report = build(&opts).unwrap();
        assert_eq!(report.features, 2);
        assert!(report.zooms[0].dropped_small >= 1, "stub omitted at z2");

        // z2 tile containing Germany: x = 2, y = 1.
        let layers = decode(&tile(&out, 2, 2, 1).expect("tile z2/2/1"));
        assert_eq!(layers[0].name, "hvlines");
        assert_eq!(layers[0].features.len(), 1);
        assert!(layers[0].features[0].properties.contains(&("voltage_kv".into(), Value::Int(380))));

        // At z6 the line runs from tile 34/21 (Berlin) into 34/22 (Munich). Both
        // copies extend into the clip buffer, so the join renders without a gap.
        let north: Vec<[i32; 2]> = decode(&tile(&out, 6, 34, 21).unwrap())[0].features[0].parts.concat();
        let south: Vec<[i32; 2]> = decode(&tile(&out, 6, 34, 22).unwrap())[0].features[0].parts.concat();
        assert!(north.iter().any(|c| c[1] > 4096), "{north:?}");
        assert!(south.iter().any(|c| c[1] < 0), "{south:?}");

        let conn = Connection::open(&out).unwrap();
        let json: String = conn.query_row("SELECT value FROM metadata WHERE name='json'", [], |r| r.get(0)).unwrap();
        let meta: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(meta["vector_layers"][0]["id"], "hvlines");
        assert_eq!(meta["vector_layers"][0]["fields"]["voltage_kv"], "Number");
        let meta = crate::routes::tiles::read_meta(&out, "net").unwrap();
        assert_eq!((meta.minzoom, meta.maxzoom), (2, 8));
    }

    #[test]
    fn oversized_tiles_keep_the_longest_lines() {
        let src = input::tests::temp("dense.geojsonl");
        let mut text = String::new();
        for i in 0..400 {
            let lat = 50.0 + i as f64 * 0.01;
            let len = if i == 7 { 5.0 } else { 0.2 };
            text.push_str(&format!(
                "{{\"type\":\"Feature\",\"geometry\":{{\"type\":\"LineString\",\"coordinates\":[[10.0,{lat}],[{},{lat}]]}},\"properties\":{{\"id\":{i}}}}}\n",
                10.0 + len
            ));
        }
        std::fs::write(&src, text).unwrap();
        let out = input::tests::temp("dense.mbtiles");
        let mut opts = BuildOptions::new(out.clone(), vec![src], "lines");
        opts.minzoom = 4;
        opts.maxzoom = 4;
        opts.max_tile_bytes = 600;
        let report = build(&opts).unwrap();
        assert!(report.zooms[0].dropped_for_size > 0);
        let layers = decode(&tile(&out, 4, 8, 5).unwrap());
        assert!(layers[0].features.iter().any(|f| f.properties.contains(&("id".into(), Value::Int(7)))), "longest line survives");
    }

    #[test]
    fn polygons_and_points_are_encoded() {
        let src = input::tests::temp("mixed.geojson");
        std::fs::write(
            &src,
            r#"{"type":"FeatureCollection","features":[
              {"type":"Feature","geometry":{"type":"Polygon","coordinates":[[[0,0],[0,10],[10,10],[10,0],[0,0]]]},"properties":{"kind":"farm"}},
              {"type":"Feature","geometry":{"type":"Point","coordinates":[5,5]},"properties":{"kind":"turbine"}}
            ]}"#,
        )
        .unwrap();
        let out = input::tests::temp("mixed.mbtiles");
        let mut opts = BuildOptions::new(out.clone(), vec![src], "energy");
        opts.minzoom = 3;
        opts.maxzoom = 3;
        build(&opts).unwrap();
        let layers = decode(&tile(&out, 3, 4, 3).unwrap());
        let types: HashSet<u32> = layers[0].features.iter().map(|f| f.geom_type).collect();
        assert_eq!(types, [1, 3].into());
        let polygon = layers[0].features.iter().find(|f| f.geom_type == 3).unwrap();
        assert!(ring_area(&polygon.parts[0]) > 0, "exterior ring is clockwise");
    }

    #[test]
    fn touching_segments_are_merged_in_either_direction() {
        let parts = vec![vec![[0, 0], [10, 0]], vec![[20, 0], [10, 0]], vec![[20, 0], [30, 5]], vec![[100, 100], [110, 110]]];
        let mut merged = merge_lines(parts);
        merged.sort_by_key(|p| p.len());
        assert_eq!(merged.len(), 2);
        let long = &merged[1];
        assert_eq!(long.len(), 4);
        assert!(long.contains(&[0, 0]) && long.contains(&[30, 5]));
        assert_eq!(merge_lines(vec![vec![[1, 1], [2, 2]]]).len(), 1);
    }

    #[test]
    fn gpkg_inputs_build() {
        let gpkg = input::tests::temp("build.gpkg");
        input::tests::write_gpkg(&gpkg, 4326);
        let out = input::tests::temp("gpkg.mbtiles");
        let mut opts = BuildOptions::new(out.clone(), vec![gpkg], "grid");
        opts.maxzoom = 5;
        let report = build(&opts).unwrap();
        assert_eq!(report.features, 1);
        assert!(report.tiles() >= 6);
    }
}
