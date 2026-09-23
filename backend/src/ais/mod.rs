//! Live AIS state: vessel positions, static voyage data and aids to navigation.
//!
//! The stream client feeds [`AisHub::apply`]; a 1 s ticker drains changed
//! vessels into one compact batch that is serialised once and broadcast to
//! every WebSocket client.

pub mod parse;
pub mod stream;

use dashmap::DashMap;
use serde::Serialize;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

pub use parse::{parse_message, PositionUpdate, Update};

/// Positions older than this are removed from the live picture.
pub const MAX_POSITION_AGE_SECS: i64 = 30 * 60;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShipKind {
    #[default]
    Vessel,
    SarAircraft,
}

impl ShipKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ShipKind::Vessel => "vessel",
            ShipKind::SarAircraft => "sar",
        }
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Ship {
    pub mmsi: u64,
    pub lat: f64,
    pub lon: f64,
    pub course: Option<f64>,
    pub speed: Option<f64>,
    pub heading: Option<f64>,
    pub rate_of_turn: Option<i32>,
    pub nav_status: Option<u8>,
    pub name: String,
    pub ship_type: Option<u32>,
    pub imo: Option<u64>,
    pub callsign: Option<String>,
    pub destination: Option<String>,
    pub eta: Option<String>,
    pub draught: Option<f64>,
    pub length: Option<u32>,
    pub beam: Option<u32>,
    pub kind: ShipKind,
    pub altitude: Option<f64>,
    /// Time of the last position report (Unix seconds).
    pub timestamp: i64,
}

/// Voyage and identity data that arrives separately from positions.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StaticInfo {
    pub name: Option<String>,
    pub ship_type: Option<u32>,
    pub imo: Option<u64>,
    pub callsign: Option<String>,
    pub destination: Option<String>,
    pub eta: Option<String>,
    pub draught: Option<f64>,
    pub length_beam: Option<(u32, u32)>,
}

impl StaticInfo {
    fn apply_to(&self, ship: &mut Ship) {
        if let Some(v) = &self.name {
            ship.name = v.clone();
        }
        if self.ship_type.is_some() {
            ship.ship_type = self.ship_type;
        }
        if self.imo.is_some() {
            ship.imo = self.imo;
        }
        if self.callsign.is_some() {
            ship.callsign = self.callsign.clone();
        }
        if self.destination.is_some() {
            ship.destination = self.destination.clone();
        }
        if self.eta.is_some() {
            ship.eta = self.eta.clone();
        }
        if self.draught.is_some() {
            ship.draught = self.draught;
        }
        if let Some((length, beam)) = self.length_beam {
            ship.length = Some(length);
            ship.beam = (beam > 0).then_some(beam);
        }
    }

    fn merge(&mut self, newer: StaticInfo) {
        macro_rules! take {
            ($($f:ident),*) => { $( if newer.$f.is_some() { self.$f = newer.$f; } )* };
        }
        take!(name, ship_type, imo, callsign, destination, eta, draught, length_beam);
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AtoN {
    pub mmsi: u64,
    pub lat: f64,
    pub lon: f64,
    pub name: String,
    pub aton_type: u32,
    pub virtual_aton: bool,
    pub off_position: bool,
    pub timestamp: i64,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct AisStatus {
    pub configured: bool,
    pub connected: bool,
    pub connected_since: Option<i64>,
    pub last_message_at: Option<i64>,
    pub messages: u64,
    pub last_error: Option<String>,
}

/// Column order of the compact row format used by the WebSocket and snapshot.
pub const ROW_FIELDS: [&str; 11] =
    ["mmsi", "lon", "lat", "course", "speed", "heading", "ship_type", "nav_status", "timestamp", "name", "kind"];

type Row<'a> = (u64, f64, f64, Option<f64>, Option<f64>, Option<f64>, Option<u32>, Option<u8>, i64, &'a str, u8);

fn round(v: f64, places: i32) -> f64 {
    let f = 10f64.powi(places);
    (v * f).round() / f
}

fn row(ship: &Ship) -> Row<'_> {
    (
        ship.mmsi,
        round(ship.lon, 5),
        round(ship.lat, 5),
        ship.course.map(|v| round(v, 1)),
        ship.speed.map(|v| round(v, 1)),
        ship.heading,
        ship.ship_type,
        ship.nav_status,
        ship.timestamp,
        &ship.name,
        (ship.kind == ShipKind::SarAircraft) as u8,
    )
}

#[derive(Serialize)]
struct Batch<'a> {
    r#type: &'a str,
    ts: i64,
    fields: [&'static str; 11],
    rows: Vec<Row<'a>>,
    removed: Vec<u64>,
}

pub struct AisHub {
    pub ships: DashMap<u64, Ship>,
    pending_static: DashMap<u64, (StaticInfo, i64)>,
    pub aton: DashMap<u64, AtoN>,
    dirty: Mutex<HashSet<u64>>,
    removed: Mutex<Vec<u64>>,
    pub updates: broadcast::Sender<Arc<str>>,
    pub status: Mutex<AisStatus>,
}

impl AisHub {
    pub fn new(configured: bool) -> Arc<Self> {
        let (updates, _) = broadcast::channel(64);
        Arc::new(Self {
            ships: DashMap::new(),
            pending_static: DashMap::new(),
            aton: DashMap::new(),
            dirty: Mutex::new(HashSet::new()),
            removed: Mutex::new(Vec::new()),
            updates,
            status: Mutex::new(AisStatus { configured, ..AisStatus::default() }),
        })
    }

    pub fn seed(&self, ships: Vec<Ship>) {
        for ship in ships {
            self.ships.insert(ship.mmsi, ship);
        }
    }

    fn mark_dirty(&self, mmsi: u64) {
        self.dirty.lock().unwrap_or_else(|e| e.into_inner()).insert(mmsi);
    }

    pub fn apply(&self, update: Update) {
        self.apply_at(update, chrono::Utc::now().timestamp());
    }

    pub fn apply_at(&self, update: Update, now: i64) {
        match update {
            Update::Position(p, info) => {
                let pending = self.pending_static.remove(&p.mmsi).map(|(_, (info, _))| info);
                let mut entry = self.ships.entry(p.mmsi).or_insert_with(|| Ship { mmsi: p.mmsi, ..Ship::default() });
                let ship = entry.value_mut();
                ship.kind = ShipKind::Vessel;
                ship.lat = p.lat;
                ship.lon = p.lon;
                ship.course = p.course;
                ship.speed = p.speed;
                ship.heading = p.heading;
                if p.nav_status.is_some() {
                    ship.nav_status = p.nav_status;
                }
                ship.rate_of_turn = p.rate_of_turn;
                if ship.name.is_empty() {
                    if let Some(name) = p.name {
                        ship.name = name;
                    }
                }
                if let Some(info) = pending {
                    info.apply_to(ship);
                }
                if let Some(info) = info {
                    info.apply_to(ship);
                }
                ship.timestamp = now;
                drop(entry);
                self.mark_dirty(p.mmsi);
            }
            Update::Static(mmsi, info) => {
                if let Some(mut ship) = self.ships.get_mut(&mmsi) {
                    info.apply_to(&mut ship);
                    drop(ship);
                    self.mark_dirty(mmsi);
                } else {
                    // Keep voyage data until the vessel reports a position.
                    let mut entry = self.pending_static.entry(mmsi).or_insert_with(|| (StaticInfo::default(), now));
                    entry.0.merge(info);
                    entry.1 = now;
                }
            }
            Update::SarAircraft { mmsi, lat, lon, altitude, speed, course } => {
                let mut entry = self.ships.entry(mmsi).or_insert_with(|| Ship { mmsi, ..Ship::default() });
                let ship = entry.value_mut();
                ship.kind = ShipKind::SarAircraft;
                ship.lat = lat;
                ship.lon = lon;
                ship.altitude = altitude;
                ship.speed = speed;
                ship.course = course;
                if ship.name.is_empty() {
                    ship.name = format!("SAR aircraft {mmsi}");
                }
                ship.timestamp = now;
                drop(entry);
                self.mark_dirty(mmsi);
            }
            Update::AtoN(aton) => {
                self.aton.insert(aton.mmsi, aton);
            }
            Update::Error(_) => {}
        }
    }

    /// Serialise vessels changed since the last call, or `None` if nothing changed.
    pub fn flush_batch(&self) -> Option<Arc<str>> {
        let ids: Vec<u64> = std::mem::take(&mut *self.dirty.lock().unwrap_or_else(|e| e.into_inner())).into_iter().collect();
        let removed = std::mem::take(&mut *self.removed.lock().unwrap_or_else(|e| e.into_inner()));
        if ids.is_empty() && removed.is_empty() {
            return None;
        }
        let guards: Vec<_> = ids.iter().filter_map(|id| self.ships.get(id)).collect();
        let batch = Batch {
            r#type: "ships",
            ts: chrono::Utc::now().timestamp(),
            fields: ROW_FIELDS,
            rows: guards.iter().map(|g| row(g.value())).collect(),
            removed,
        };
        let json = serde_json::to_string(&batch).ok()?;
        Some(Arc::from(json))
    }

    /// All live vessels in the compact row format.
    pub fn snapshot_json(&self) -> String {
        let guards: Vec<_> = self.ships.iter().collect();
        let batch = Batch {
            r#type: "snapshot",
            ts: chrono::Utc::now().timestamp(),
            fields: ROW_FIELDS,
            rows: guards.iter().map(|g| row(g.value())).collect(),
            removed: Vec::new(),
        };
        serde_json::to_string(&batch).unwrap_or_else(|_| "{}".into())
    }

    /// Drop stale positions, stale pending voyage data and stale navigation aids.
    pub fn prune(&self, now: i64) -> usize {
        let cutoff = now - MAX_POSITION_AGE_SECS;
        let mut gone = Vec::new();
        self.ships.retain(|mmsi, ship| {
            let keep = ship.timestamp >= cutoff;
            if !keep {
                gone.push(*mmsi);
            }
            keep
        });
        self.pending_static.retain(|_, (_, at)| *at >= now - 2 * 3600);
        self.aton.retain(|_, a| a.timestamp >= now - 6 * 3600);
        let count = gone.len();
        if count > 0 {
            self.removed.lock().unwrap_or_else(|e| e.into_inner()).extend(gone);
        }
        count
    }

    pub fn ships_updated_since(&self, since: i64) -> Vec<Ship> {
        self.ships.iter().filter(|s| s.timestamp >= since).map(|s| s.value().clone()).collect()
    }

    pub fn all_ships(&self) -> Vec<Ship> {
        self.ships.iter().map(|s| s.value().clone()).collect()
    }

    pub fn status(&self) -> AisStatus {
        self.status.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn update_status(&self, f: impl FnOnce(&mut AisStatus)) {
        f(&mut self.status.lock().unwrap_or_else(|e| e.into_inner()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn position(mmsi: u64, lat: f64) -> Update {
        Update::Position(
            PositionUpdate {
                mmsi,
                lat,
                lon: 10.0,
                name: Some("META NAME".into()),
                course: Some(90.0),
                speed: Some(12.0),
                heading: None,
                nav_status: Some(0),
                rate_of_turn: None,
            },
            None,
        )
    }

    #[test]
    fn static_data_before_a_position_never_creates_a_vessel() {
        let hub = AisHub::new(true);
        hub.apply_at(Update::Static(7, StaticInfo { name: Some("EARLY".into()), ship_type: Some(80), ..Default::default() }), 100);
        assert!(hub.ships.is_empty());
        assert!(hub.flush_batch().is_none());
        hub.apply_at(position(7, 54.0), 101);
        let ship = hub.ships.get(&7).unwrap().clone();
        assert_eq!(ship.name, "EARLY");
        assert_eq!(ship.ship_type, Some(80));
        assert_eq!(ship.lat, 54.0);
    }

    #[test]
    fn static_updates_are_merged_and_published() {
        let hub = AisHub::new(true);
        hub.apply_at(position(1, 50.0), 100);
        hub.flush_batch();
        hub.apply_at(Update::Static(1, StaticInfo { ship_type: Some(70), length_beam: Some((200, 0)), ..Default::default() }), 101);
        let batch = hub.flush_batch().expect("type change is published");
        assert!(batch.contains("\"rows\":[[1,10.0,50.0,90.0,12.0,null,70,0,100,\"META NAME\",0]]"), "{batch}");
        let ship = hub.ships.get(&1).unwrap().clone();
        assert_eq!((ship.length, ship.beam), (Some(200), None));
        assert!(hub.flush_batch().is_none());
    }

    #[test]
    fn batches_deduplicate_and_report_removals() {
        let hub = AisHub::new(true);
        for i in 0..5 {
            hub.apply_at(position(3, 50.0 + i as f64), 1_000);
        }
        let batch: serde_json::Value = serde_json::from_str(&hub.flush_batch().unwrap()).unwrap();
        assert_eq!(batch["rows"].as_array().unwrap().len(), 1);
        assert_eq!(batch["rows"][0][2], 54.0);
        assert_eq!(hub.prune(1_000 + MAX_POSITION_AGE_SECS + 1), 1);
        let batch: serde_json::Value = serde_json::from_str(&hub.flush_batch().unwrap()).unwrap();
        assert_eq!(batch["removed"], serde_json::json!([3]));
        assert!(hub.ships.is_empty());
    }

    #[test]
    fn snapshot_lists_every_vessel_in_row_format() {
        let hub = AisHub::new(true);
        hub.apply_at(position(1, 50.0), 10);
        hub.apply_at(Update::SarAircraft { mmsi: 111_232_000, lat: 1.0, lon: 2.0, altitude: Some(300.0), speed: None, course: None }, 10);
        let snap: serde_json::Value = serde_json::from_str(&hub.snapshot_json()).unwrap();
        assert_eq!(snap["type"], "snapshot");
        assert_eq!(snap["fields"].as_array().unwrap().len(), ROW_FIELDS.len());
        assert_eq!(snap["rows"].as_array().unwrap().len(), 2);
        let sar = snap["rows"].as_array().unwrap().iter().find(|r| r[0] == 111_232_000).unwrap();
        assert_eq!(sar[10], 1);
    }
}
