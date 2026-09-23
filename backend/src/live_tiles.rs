//! Live vector tiles for the OpenStreetMap network layers.
//!
//! When `data/tiles/<id>.mbtiles` has not been built, `/tiles/<id>/{z}/{x}/{y}`
//! is answered from Overpass instead: the area of the zoom-8 tile containing
//! the request is downloaded once, cached for a month, and every tile inside it
//! is cut from that download on demand. Below zoom 8 an optional small
//! `<id>-backbone.mbtiles` (for example lines of 300 kV and above) is served;
//! without it those zooms stay empty and the UI asks the user to zoom in.

use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use serde_json::Value as Json;

use crate::state::AppState;
use crate::tilegen::geometry::{project, split_antimeridian, P};
use crate::tilegen::input::Geometry;
use crate::tilegen::mvt::{Properties, TileFeature, Value, EXTENT};
use crate::tilegen::{bbox_of, clip_to_tile, encode_limited, simplify_geometry};
use crate::upstream::{self, UpstreamError};

/// Zoom of the areas downloaded from Overpass (about 1.4° wide): small enough
/// that each query stays short and the map fills in area by area.
pub const FETCH_ZOOM: u32 = 8;
/// Highest zoom served; MapLibre over-zooms beyond it.
pub const MAX_ZOOM: u32 = 12;
const CACHE_TTL_SECS: i64 = 30 * 86_400;
const OVERPASS_TIMEOUT_SECS: u64 = 60;

type Tags = serde_json::Map<String, Json>;
/// Tile attributes and a priority (bigger lines survive tile thinning longer).
type Attributes = (Vec<(Arc<str>, Value)>, f64);

pub struct LiveLayer {
    /// Tileset id used in `/tiles/<id>/…`.
    pub id: &'static str,
    /// Vector layer name inside the tiles (matches the prebuilt tilesets).
    pub layer: &'static str,
    query: fn(&str) -> String,
    properties: fn(&Tags) -> Option<Attributes>,
}

pub static LAYERS: [LiveLayer; 2] = [
    LiveLayer { id: "hv-lines", layer: "hvlines", query: hv_query, properties: hv_properties },
    LiveLayer { id: "pipelines", layer: "pipelines", query: pipeline_query, properties: pipeline_properties },
];

pub fn find(id: &str) -> Option<&'static LiveLayer> {
    LAYERS.iter().find(|l| l.id == id)
}

// ── Queries and attributes (kept in step with scripts/build_tiles.py) ─────

/// power=line/cable with any listed voltage >= 110 kV ("380000;220000" style lists).
const HV_REGEX: &str = "(^|;)(1[1-9][0-9]{4}|[2-9][0-9]{5}|[1-9][0-9]{6,})(;|$)";

fn hv_query(bbox: &str) -> String {
    format!(
        "[out:json][timeout:{OVERPASS_TIMEOUT_SECS}];(way[\"power\"=\"line\"][\"voltage\"~\"{HV_REGEX}\"]({bbox});\
         way[\"power\"=\"cable\"][\"voltage\"~\"{HV_REGEX}\"]({bbox}););out tags geom;"
    )
}

fn pipeline_query(bbox: &str) -> String {
    format!(
        "[out:json][timeout:{OVERPASS_TIMEOUT_SECS}];(\
         way[\"man_made\"=\"pipeline\"][\"substance\"~\"gas|oil|fuel|petroleum|lng|lpg|cng|hydrogen|condensate|kerosene|diesel|naphtha\",i][\"usage\"!~\"^(distribution|branch)$\"]({bbox});\
         way[\"man_made\"=\"pipeline\"][\"type\"~\"^(gas|oil|fuel|petroleum)$\"][\"usage\"!~\"^(distribution|branch)$\"]({bbox}););out tags geom;"
    )
}

fn number(value: Option<&str>) -> Option<f64> {
    let s = value?;
    let start = s.find(|c: char| c.is_ascii_digit())?;
    let digits: String = s[start..].chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    digits.trim_end_matches('.').parse().ok()
}

fn tag<'a>(tags: &'a Tags, key: &str) -> Option<&'a str> {
    tags.get(key).and_then(Json::as_str).map(str::trim).filter(|s| !s.is_empty())
}

fn push_str(props: &mut Vec<(Arc<str>, Value)>, key: &str, value: Option<&str>) {
    if let Some(v) = value {
        props.push((Arc::from(key), Value::String(v.to_string())));
    }
}

fn hv_properties(tags: &Tags) -> Option<Attributes> {
    let kv: Vec<i64> = tag(tags, "voltage")?
        .split(';')
        .filter_map(|p| p.trim().parse::<i64>().ok())
        .filter(|v| *v >= 1000)
        .map(|v| (v as f64 / 1000.0).round() as i64)
        .collect();
    let top = *kv.iter().max().filter(|v| **v >= 110)?;
    let hvdc = tag(tags, "frequency") == Some("0") || tag(tags, "line").is_some_and(|l| l.to_lowercase().contains("hvdc"));
    let mut props = vec![
        (Arc::from("voltage_kv"), Value::Int(top)),
        (Arc::from("kind"), Value::String(if tag(tags, "power") == Some("cable") { "cable" } else { "line" }.into())),
        (Arc::from("hvdc"), Value::Bool(hvdc)),
    ];
    push_str(&mut props, "voltage", tag(tags, "voltage"));
    if let Some(c) = number(tag(tags, "circuits")).map(|c| c as i64).filter(|c| *c > 0) {
        props.push((Arc::from("circuits"), Value::Int(c)));
    }
    for key in ["name", "operator", "ref", "location"] {
        push_str(&mut props, key, tag(tags, key));
    }
    Some((props, top as f64))
}

fn commodity(substance: &str) -> &'static str {
    let s = substance.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| s.contains(w));
    let gas = has(&["gas", "lng", "lpg", "cng", "methane"]);
    let oil = has(&["oil", "crude", "petroleum", "fuel", "diesel", "kerosene", "condensate", "gasoline", "naphtha"]);
    match () {
        _ if s.contains("hydrogen") => "HYDROGEN",
        _ if gas && oil && !s.contains("gasoline") => "OIL AND GAS",
        _ if oil => "OIL",
        _ if gas => "GAS",
        _ => "OTHER",
    }
}

fn pipeline_properties(tags: &Tags) -> Option<Attributes> {
    let substance = tag(tags, "substance").or_else(|| tag(tags, "type")).unwrap_or("");
    let diameter = number(tag(tags, "diameter")).filter(|d| *d > 0.0);
    let mut props = vec![(Arc::from("commodity"), Value::String(commodity(substance).into()))];
    push_str(&mut props, "substance", Some(substance).filter(|s| !s.is_empty()));
    for key in ["name", "operator", "location", "usage"] {
        push_str(&mut props, key, tag(tags, key));
    }
    if let Some(d) = diameter {
        props.push((Arc::from("diameter_mm"), Value::Int(d.round() as i64)));
    }
    Some((props, diameter.unwrap_or(0.0)))
}

// ── Downloading areas ──────────────────────────────────────────────────────

/// Public Overpass instances (wiki.openstreetmap.org/wiki/Overpass_API);
/// `OVERPASS_URL` is tried first when set.
fn overpass_urls() -> &'static [String] {
    static URLS: LazyLock<Vec<String>> = LazyLock::new(|| {
        let mut urls: Vec<String> = std::env::var("OVERPASS_URL").ok().filter(|u| !u.trim().is_empty()).into_iter().collect();
        for u in [
            "https://overpass-api.de/api/interpreter",
            "https://maps.mail.ru/osm/tools/overpass/api/interpreter",
            "https://overpass.private.coffee/api/interpreter",
        ] {
            if !urls.iter().any(|x| x == u) {
                urls.push(u.to_string());
            }
        }
        urls
    });
    &URLS
}

/// Keeps a handful of downloads in flight, spread over the servers.
static SLOTS: LazyLock<tokio::sync::Semaphore> = LazyLock::new(|| tokio::sync::Semaphore::new(6));
static NEXT_SERVER: AtomicUsize = AtomicUsize::new(0);

/// Bounding box (south, west, north, east) of a fetch-zoom tile.
fn area_bbox(x: u32, y: u32) -> String {
    let n = (1u32 << FETCH_ZOOM) as f64;
    let lon = |x: f64| x / n * 360.0 - 180.0;
    let lat = |y: f64| (std::f64::consts::PI * (1.0 - 2.0 * y / n)).sinh().atan().to_degrees();
    format!("{:.5},{:.5},{:.5},{:.5}", lat(y as f64 + 1.0), lon(x as f64), lat(y as f64), lon(x as f64 + 1.0))
}

async fn overpass(state: &AppState, query: String) -> Result<Vec<u8>, UpstreamError> {
    let _slot = SLOTS.acquire().await.map_err(|e| UpstreamError::Network(e.to_string()))?;
    let urls = overpass_urls();
    let start = NEXT_SERVER.fetch_add(1, Ordering::Relaxed);
    let mut last = UpstreamError::Network("no Overpass server configured".into());
    for i in 0..urls.len() {
        let url = &urls[(start + i) % urls.len()];
        let request =
            state.http.post(url).timeout(std::time::Duration::from_secs(OVERPASS_TIMEOUT_SECS + 15)).form(&[("data", query.as_str())]);
        match upstream::send(request).await {
            Ok(body) => {
                // Overpass reports server-side timeouts inside a 200 response.
                let head = String::from_utf8_lossy(&body[..body.len().min(2048)]).to_string();
                if head.contains("\"remark\"")
                    && (head.contains("runtime error") || head.contains("timed out") || head.contains("out of memory"))
                {
                    last = UpstreamError::Invalid("Overpass query timed out".into());
                    continue;
                }
                return Ok(body);
            }
            Err(e) => {
                tracing::debug!("Overpass {url} failed: {e}");
                last = e;
            }
        }
    }
    Err(last)
}

/// Tags read by the attribute functions; everything else is dropped before caching.
const KEPT_TAGS: &[&str] =
    &["power", "voltage", "frequency", "line", "circuits", "name", "operator", "ref", "location", "substance", "type", "diameter", "usage"];

/// Convert an Overpass response into compact `[[props, [[lon,lat],…]], …]`, gzipped.
fn compact(layer: &LiveLayer, body: &[u8]) -> Result<Vec<u8>, UpstreamError> {
    let json: Json = serde_json::from_slice(body).map_err(|e| UpstreamError::Invalid(e.to_string()))?;
    let mut out = Vec::new();
    for el in json.get("elements").and_then(Json::as_array).into_iter().flatten() {
        if el.get("type").and_then(Json::as_str) != Some("way") {
            continue;
        }
        let Some(tags) = el.get("tags").and_then(Json::as_object) else { continue };
        let coords: Vec<Json> = el
            .get("geometry")
            .and_then(Json::as_array)
            .into_iter()
            .flatten()
            .filter_map(|p| Some(serde_json::json!([p.get("lon")?.as_f64()?, p.get("lat")?.as_f64()?])))
            .collect();
        if coords.len() < 2 || (layer.properties)(tags).is_none() {
            continue;
        }
        let kept: Tags = tags.iter().filter(|(k, _)| KEPT_TAGS.contains(&k.as_str())).map(|(k, v)| (k.clone(), v.clone())).collect();
        out.push(serde_json::json!([kept, coords]));
    }
    let raw = serde_json::to_vec(&out).map_err(|e| UpstreamError::Invalid(e.to_string()))?;
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    encoder.write_all(&raw).and_then(|_| encoder.finish()).map_err(|e| UpstreamError::Invalid(e.to_string()))
}

struct AreaFeature {
    geometry: Geometry,
    bbox: [f64; 4],
    properties: Properties,
    priority: f64,
}

type Area = Arc<Vec<AreaFeature>>;

fn parse_area(layer: &LiveLayer, gz: &[u8]) -> anyhow::Result<Area> {
    let mut raw = Vec::new();
    flate2::read::GzDecoder::new(gz).read_to_end(&mut raw)?;
    let rows: Vec<(Tags, Vec<(f64, f64)>)> = serde_json::from_slice(&raw)?;
    let mut features = Vec::with_capacity(rows.len());
    for (tags, coords) in rows {
        let Some((props, priority)) = (layer.properties)(&tags) else { continue };
        let lines: Vec<Vec<P>> = split_antimeridian(coords)
            .into_iter()
            .map(|l| l.into_iter().map(|(lon, lat)| project(lon, lat)).collect::<Vec<P>>())
            .filter(|l| l.len() >= 2)
            .collect();
        if lines.is_empty() {
            continue;
        }
        let geometry = Geometry::Lines(lines);
        features.push(AreaFeature { bbox: bbox_of(&geometry), geometry, properties: Arc::new(props), priority });
    }
    Ok(Arc::new(features))
}

/// Recently used areas, parsed, so neighbouring tiles do not re-read the cache.
type AreaCache = (HashMap<String, Area>, VecDeque<String>);
static AREAS: LazyLock<Mutex<AreaCache>> = LazyLock::new(|| Mutex::new((HashMap::new(), VecDeque::new())));
const AREAS_KEPT: usize = 48;

async fn area(state: &Arc<AppState>, layer: &'static LiveLayer, x: u32, y: u32) -> Result<Area, UpstreamError> {
    let key = format!("live:{}:{FETCH_ZOOM}/{x}/{y}", layer.id);
    if let Some(a) = AREAS.lock().unwrap_or_else(|e| e.into_inner()).0.get(&key) {
        return Ok(a.clone());
    }
    let query = (layer.query)(&area_bbox(x, y));
    let fetch_state = state.clone();
    let fetched = upstream::cached(&state.cache_db, &state.providers.overpass, &key, CACHE_TTL_SECS, || async move {
        let started = std::time::Instant::now();
        let body = overpass(&fetch_state, query).await?;
        let packed = compact(layer, &body)?;
        tracing::info!(
            "Live {} area {FETCH_ZOOM}/{x}/{y}: {} KB in {:.1}s",
            layer.id,
            packed.len() / 1024,
            started.elapsed().as_secs_f64()
        );
        Ok(packed)
    })
    .await?;
    let parsed = tokio::task::spawn_blocking(move || parse_area(layer, &fetched.body))
        .await
        .map_err(|e| UpstreamError::Invalid(e.to_string()))?
        .map_err(|e| UpstreamError::Invalid(e.to_string()))?;
    let mut cache = AREAS.lock().unwrap_or_else(|e| e.into_inner());
    let (map, order) = &mut *cache;
    if map.insert(key.clone(), parsed.clone()).is_none() {
        order.push_back(key);
        while order.len() > AREAS_KEPT {
            if let Some(old) = order.pop_front() {
                map.remove(&old);
            }
        }
    }
    Ok(parsed)
}

// ── Tiles ──────────────────────────────────────────────────────────────────

fn encode(layer: &LiveLayer, features: &[AreaFeature], z: u32, x: u32, y: u32) -> Option<Vec<u8>> {
    let n = (1u64 << z) as f64;
    let tolerance = if z >= MAX_ZOOM { 0.0 } else { 1.0 / (n * EXTENT as f64) };
    let buffer = 64.0 / EXTENT as f64;
    let (b, x0, y0) = (buffer / n, x as f64 / n, y as f64 / n);
    let rect = [x0 - b, y0 - b, x0 + 1.0 / n + b, y0 + 1.0 / n + b];
    let mut out = Vec::new();
    for f in features {
        if f.bbox[2] < rect[0] || f.bbox[0] > rect[2] || f.bbox[3] < rect[1] || f.bbox[1] > rect[3] {
            continue;
        }
        let Some(g) = simplify_geometry(&f.geometry, tolerance, 0.0) else { continue };
        if let Some((geom_type, parts)) = clip_to_tile(&g, &f.bbox, n, x, y, buffer) {
            out.push(TileFeature { geom_type, parts, properties: f.properties.clone(), priority: f.priority });
        }
    }
    (!out.is_empty()).then(|| encode_limited(layer.layer, out, 500 * 1024, true).0)
}

/// A gzipped vector tile, `Ok(None)` for an empty tile.
pub async fn tile(state: &Arc<AppState>, layer: &'static LiveLayer, z: u32, x: u32, y: u32) -> Result<Option<Vec<u8>>, UpstreamError> {
    let shift = z - FETCH_ZOOM;
    let features = area(state, layer, x >> shift, y >> shift).await?;
    tokio::task::spawn_blocking(move || encode(layer, &features, z, x, y)).await.map_err(|e| UpstreamError::Invalid(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tilegen::mvt::decode_tile;

    fn gz(json: &Json) -> Vec<u8> {
        let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        e.write_all(json.to_string().as_bytes()).unwrap();
        e.finish().unwrap()
    }

    #[test]
    fn overpass_ways_become_tiles() {
        let layer = find("hv-lines").unwrap();
        let body = serde_json::json!({ "elements": [
            { "type": "way", "id": 1, "tags": { "power": "line", "voltage": "380000;220000", "name": "North" },
              "geometry": [{ "lat": 50.0, "lon": 8.0 }, { "lat": 50.5, "lon": 8.6 }] },
            { "type": "way", "id": 2, "tags": { "power": "line", "voltage": "20000" },
              "geometry": [{ "lat": 50.0, "lon": 8.0 }, { "lat": 50.1, "lon": 8.1 }] },
            { "type": "node", "id": 3, "lat": 50.0, "lon": 8.0 }
        ]});
        let packed = compact(layer, body.to_string().as_bytes()).unwrap();
        let area = parse_area(layer, &packed).unwrap();
        assert_eq!(area.len(), 1, "only the 380 kV line is kept");

        // z10 tile containing 50.2N 8.3E.
        let [px, py] = project(8.3, 50.2);
        let (x, y) = ((px * 1024.0) as u32, (py * 1024.0) as u32);
        let tile = encode(layer, &area, 10, x, y).expect("tile has the line");
        let mut raw = Vec::new();
        flate2::read::GzDecoder::new(&tile[..]).read_to_end(&mut raw).unwrap();
        let layers = decode_tile(&raw).unwrap();
        assert_eq!(layers[0].name, "hvlines");
        let props = &layers[0].features[0].properties;
        assert!(props.iter().any(|(k, v)| k == "voltage_kv" && *v == Value::Int(380)));
        assert!(encode(layer, &area, 10, x + 20, y).is_none());
    }

    #[test]
    fn cached_areas_parse_and_bboxes_cover_fetch_tiles() {
        let layer = find("pipelines").unwrap();
        let rows =
            serde_json::json!([[{ "man_made": "pipeline", "substance": "gas", "diameter": "1200 mm" }, [[10.0, 54.0], [11.0, 54.5]]]]);
        let area = parse_area(layer, &gz(&rows)).unwrap();
        assert_eq!(area.len(), 1);
        assert!(area[0].properties.iter().any(|(k, v)| &**k == "commodity" && *v == Value::String("GAS".into())));
        assert!(area[0].properties.iter().any(|(k, v)| &**k == "diameter_mm" && *v == Value::Int(1200)));
        assert_eq!(area_bbox(128, 127), "0.00000,0.00000,1.40611,1.40625");
    }

    #[test]
    fn commodities_match_the_build_script() {
        assert_eq!(commodity("Natural gas"), "GAS");
        assert_eq!(commodity("crude_oil"), "OIL");
        assert_eq!(commodity("oil;gas"), "OIL AND GAS");
        assert_eq!(commodity("gasoline"), "OIL");
        assert_eq!(commodity("hydrogen"), "HYDROGEN");
        assert_eq!(commodity("water"), "OTHER");
        assert_eq!(number(Some("DN 800")), Some(800.0));
    }
}
