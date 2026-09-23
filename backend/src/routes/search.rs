//! Unified search across local datasets, live vessels and (on request) places.
//!
//! Local matches are instant and suitable for search-as-you-type. Place search
//! uses Nominatim, whose usage policy forbids autocomplete, so the client only
//! asks for places when the user submits the query.

use axum::extract::{Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

use super::{ApiError, ApiResult};
use crate::ais::AisHub;
use crate::datasets::Datasets;
use crate::state::AppState;
use crate::upstream::{self, UpstreamError};

const USER_AGENT: &str = concat!("WorldMap/", env!("CARGO_PKG_VERSION"), " (+https://github.com/leonsuv/worldmap)");

#[derive(Deserialize)]
pub struct SearchQuery {
    q: String,
    /// Include Nominatim place results (default true).
    places: Option<bool>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SearchResult {
    pub kind: &'static str,
    pub id: String,
    pub name: String,
    pub detail: String,
    pub lat: f64,
    pub lon: f64,
    pub zoom: f64,
}

fn score_text(query: &str, text: &str) -> u32 {
    let text = text.to_lowercase();
    if text == query {
        70
    } else if text.starts_with(query) {
        55
    } else if text.split(|c: char| !c.is_alphanumeric()).any(|w| w.starts_with(query)) {
        40
    } else if query.len() >= 3 && text.contains(query) {
        20
    } else {
        0
    }
}

fn code_match(query: &str, code: Option<&str>) -> bool {
    code.is_some_and(|c| !c.is_empty() && c.eq_ignore_ascii_case(query))
}

pub fn local_search(ds: &Datasets, ais: &AisHub, raw: &str, limit: usize) -> Vec<SearchResult> {
    let q = raw.trim().to_lowercase();
    if q.chars().count() < 2 {
        return Vec::new();
    }
    let mut hits: Vec<(u32, SearchResult)> = Vec::new();

    for a in &ds.airports {
        let code = if code_match(&q, a.iata.as_deref()) || code_match(&q, Some(&a.ident)) { 100 } else { 0 };
        let score = code.max(score_text(&q, &a.name)).max(a.city.as_deref().map_or(0, |c| score_text(&q, c).saturating_sub(5)));
        if score > 0 {
            let bonus = match a.kind.as_str() {
                "large" => 12,
                "medium" => 5,
                _ => 0,
            };
            let codes =
                [a.iata.as_deref(), Some(a.ident.as_str())].into_iter().flatten().filter(|c| !c.is_empty()).collect::<Vec<_>>().join(" · ");
            let place = [a.city.as_deref(), a.country.as_deref()].into_iter().flatten().collect::<Vec<_>>().join(", ");
            hits.push((
                score + bonus,
                SearchResult {
                    kind: "airport",
                    id: a.ident.clone(),
                    name: a.name.clone(),
                    detail: [codes, place].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" — "),
                    lat: a.lat,
                    lon: a.lon,
                    zoom: 11.0,
                },
            ));
        }
    }
    for p in &ds.seaports {
        let code = if code_match(&q, p.locode.as_deref()) { 100 } else { 0 };
        let score = code.max(score_text(&q, &p.name));
        if score > 0 {
            let bonus = match p.size.as_deref() {
                Some("large") => 10,
                Some("medium") => 5,
                _ => 0,
            };
            hits.push((
                score + bonus,
                SearchResult {
                    kind: "seaport",
                    id: p.locode.clone().unwrap_or_else(|| p.name.clone()),
                    name: p.name.clone(),
                    detail: [p.locode.as_deref(), p.country.as_deref()].into_iter().flatten().collect::<Vec<_>>().join(" — "),
                    lat: p.lat,
                    lon: p.lon,
                    zoom: 11.0,
                },
            ));
        }
    }
    for p in &ds.plants {
        let score = score_text(&q, &p.name);
        if score > 0 {
            hits.push((
                score,
                SearchResult {
                    kind: "reactor",
                    id: p.name.clone(),
                    name: format!("{} nuclear plant", p.name),
                    detail: format!("{} — {:.0} MW", p.country, p.capacity_mw),
                    lat: p.lat,
                    lon: p.lon,
                    zoom: 10.0,
                },
            ));
        }
    }
    if q.len() >= 3 {
        let numeric = q.parse::<u64>().ok();
        for s in ais.ships.iter() {
            let code = numeric.is_some_and(|n| n == s.mmsi || Some(n) == s.imo) || code_match(&q, s.callsign.as_deref());
            let score = if code {
                100
            } else if s.name.is_empty() {
                0
            } else {
                score_text(&q, &s.name)
            };
            if score > 0 {
                hits.push((
                    score,
                    SearchResult {
                        kind: "vessel",
                        id: s.mmsi.to_string(),
                        name: if s.name.is_empty() { format!("MMSI {}", s.mmsi) } else { s.name.clone() },
                        detail: format!("Vessel · MMSI {}", s.mmsi),
                        lat: s.lat,
                        lon: s.lon,
                        zoom: 12.0,
                    },
                ));
            }
        }
    }
    hits.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.name.len().cmp(&b.1.name.len())));
    hits.into_iter().take(limit).map(|(_, r)| r).collect()
}

/// Zoom that fits a Nominatim bounding box `[south, north, west, east]`.
fn zoom_for_bbox(bbox: Option<&Value>) -> f64 {
    let Some(b) = bbox.and_then(Value::as_array) else { return 11.0 };
    let n: Vec<f64> = b.iter().filter_map(|v| v.as_str()?.parse().ok()).collect();
    if n.len() != 4 {
        return 11.0;
    }
    let span = (n[1] - n[0]).abs().max((n[3] - n[2]).abs()).max(0.001);
    (360.0 / span).log2().clamp(2.0, 15.0).floor()
}

pub fn parse_places(data: &Value) -> Vec<SearchResult> {
    let mut seen = std::collections::HashSet::new();
    data.as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|p| {
                    let lat: f64 = p.get("lat")?.as_str()?.parse().ok()?;
                    let lon: f64 = p.get("lon")?.as_str()?.parse().ok()?;
                    let display = p.get("display_name")?.as_str()?;
                    let (name, detail) = display.split_once(", ").unwrap_or((display, ""));
                    // Nominatim often returns a city and its municipality under the same name.
                    if !seen.insert((name.to_string(), detail.to_string())) {
                        return None;
                    }
                    Some(SearchResult {
                        kind: "place",
                        id: p.get("place_id").map(|v| v.to_string()).unwrap_or_default(),
                        name: name.to_string(),
                        detail: detail.to_string(),
                        lat,
                        lon,
                        zoom: zoom_for_bbox(p.get("boundingbox")),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

// Nominatim allows one request per second per application.
static NOMINATIM_GATE: tokio::sync::Mutex<Option<std::time::Instant>> = tokio::sync::Mutex::const_new(None);

async fn places(state: &AppState, query: &str) -> Result<Vec<SearchResult>, UpstreamError> {
    let key = format!("search:v2:{}", query.to_lowercase());
    let fetched = upstream::cached(&state.cache_db, &state.providers.nominatim, &key, 7 * 86400, || async {
        let mut last = NOMINATIM_GATE.lock().await;
        if let Some(at) = *last {
            let wait = std::time::Duration::from_millis(1100).saturating_sub(at.elapsed());
            tokio::time::sleep(wait).await;
        }
        *last = Some(std::time::Instant::now());
        let request = state
            .http
            .get("https://nominatim.openstreetmap.org/search")
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .query(&[("q", query), ("format", "jsonv2"), ("limit", "6"), ("accept-language", "en")]);
        upstream::send(request).await
    })
    .await?;
    let data: Value = serde_json::from_slice(&fetched.body).map_err(|e| UpstreamError::Invalid(e.to_string()))?;
    Ok(parse_places(&data))
}

/// GET /api/search?q=...&places=true
pub async fn search(State(state): State<Arc<AppState>>, Query(q): Query<SearchQuery>) -> ApiResult<Json<Value>> {
    let query = q.q.trim();
    if query.chars().count() < 2 || query.len() > 200 {
        return Err(ApiError::bad_request("Search needs 2 to 200 characters"));
    }
    let mut results = local_search(&state.datasets(), &state.ais, query, 8);
    let mut places_error = None;
    if q.places.unwrap_or(true) {
        match places(&state, query).await {
            Ok(found) => results.extend(found),
            Err(e) => places_error = Some(ApiError::from(e).message),
        }
    }
    Ok(Json(serde_json::json!({ "results": results, "places_error": places_error })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datasets::{Airport, Seaport};

    fn datasets() -> Datasets {
        let airport = |ident: &str, iata: &str, name: &str, city: &str, kind: &str| Airport {
            ident: ident.into(),
            iata: Some(iata.into()),
            name: name.into(),
            city: Some(city.into()),
            country: Some("DE".into()),
            kind: kind.into(),
            lat: 52.0,
            lon: 13.0,
            elevation_ft: None,
        };
        Datasets {
            airports: vec![
                airport("EDDB", "BER", "Berlin Brandenburg Airport", "Berlin", "large"),
                airport("EDAZ", "", "Schönhagen Airport", "Trebbin", "small"),
                airport("KBRL", "BRL", "Southeast Iowa Regional Airport", "Burlington", "medium"),
            ],
            seaports: vec![Seaport {
                name: "Hamburg".into(),
                locode: Some("DEHAM".into()),
                country: Some("Germany".into()),
                size: Some("large".into()),
                harbor_type: None,
                lat: 53.5,
                lon: 9.9,
            }],
            ..Datasets::default()
        }
    }

    #[test]
    fn codes_rank_above_names() {
        let ais = AisHub::new(false);
        let results = local_search(&datasets(), &ais, "ber", 5);
        assert_eq!(results[0].id, "EDDB", "{results:?}");
        assert_eq!(local_search(&datasets(), &ais, "deham", 5)[0].kind, "seaport");
        assert_eq!(local_search(&datasets(), &ais, "EDDB", 5)[0].name, "Berlin Brandenburg Airport");
        assert!(local_search(&datasets(), &ais, "x", 5).is_empty());
    }

    #[test]
    fn vessels_are_found_by_name_and_mmsi() {
        let ais = AisHub::new(true);
        ais.ships.insert(
            211_234_567,
            crate::ais::Ship {
                mmsi: 211_234_567,
                name: "NORDIC STAR".into(),
                lat: 54.0,
                lon: 10.0,
                imo: Some(9_300_000),
                ..Default::default()
            },
        );
        let ds = Datasets::default();
        assert_eq!(local_search(&ds, &ais, "nordic", 5)[0].kind, "vessel");
        assert_eq!(local_search(&ds, &ais, "211234567", 5)[0].id, "211234567");
        assert_eq!(local_search(&ds, &ais, "9300000", 5).len(), 1);
    }

    #[test]
    fn nominatim_results_are_parsed_with_a_fitting_zoom() {
        let data = serde_json::json!([
            { "place_id": 1, "lat": "52.52", "lon": "13.40", "display_name": "Berlin, Germany", "boundingbox": ["52.3", "52.7", "13.0", "13.8"] },
            { "lat": "bad", "lon": "0", "display_name": "Broken" },
            { "place_id": 2, "lat": "52.5", "lon": "13.4", "display_name": "Berlin, Germany" }
        ]);
        let places = parse_places(&data);
        assert_eq!(places.len(), 1);
        assert_eq!((places[0].name.as_str(), places[0].detail.as_str()), ("Berlin", "Germany"));
        assert!((8.0..=10.0).contains(&places[0].zoom), "{}", places[0].zoom);
    }
}
