//! Parse AISstream.io JSON messages into typed updates.
//!
//! Special "not available" values defined by ITU-R M.1371 are mapped to
//! `None` here so nothing downstream has to know about them.

use serde_json::Value;

use super::{AtoN, StaticInfo};

#[derive(Debug, Clone, PartialEq)]
pub struct PositionUpdate {
    pub mmsi: u64,
    pub lat: f64,
    pub lon: f64,
    pub name: Option<String>,
    pub course: Option<f64>,
    pub speed: Option<f64>,
    pub heading: Option<f64>,
    pub nav_status: Option<u8>,
    pub rate_of_turn: Option<i32>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Update {
    Position(PositionUpdate, Option<StaticInfo>),
    Static(u64, StaticInfo),
    SarAircraft {
        mmsi: u64,
        lat: f64,
        lon: f64,
        altitude: Option<f64>,
        speed: Option<f64>,
        course: Option<f64>,
    },
    AtoN(AtoN),
    /// The stream rejected the subscription (for example an invalid API key).
    Error(String),
}

pub fn parse_message(text: &str) -> Option<Update> {
    let msg: Value = serde_json::from_str(text).ok()?;
    if let Some(error) = msg.get("error").and_then(Value::as_str) {
        return Some(Update::Error(error.to_string()));
    }
    let kind = msg.get("MessageType")?.as_str()?;
    let meta = msg.get("MetaData")?;
    let body = msg.get("Message")?.get(kind)?;
    let mmsi = meta.get("MMSI").and_then(Value::as_u64).or_else(|| body.get("UserID").and_then(Value::as_u64))?;
    if mmsi == 0 {
        return None;
    }
    let meta_name = clean_text(meta.get("ShipName"));

    match kind {
        "PositionReport" | "StandardClassBPositionReport" | "LongRangeAisBroadcastMessage" => {
            let (lat, lon) = position(body, meta)?;
            Some(Update::Position(
                PositionUpdate {
                    mmsi,
                    lat,
                    lon,
                    name: meta_name,
                    course: course(body.get("Cog")),
                    speed: speed(body.get("Sog")),
                    heading: heading(body.get("TrueHeading")),
                    nav_status: body.get("NavigationalStatus").and_then(Value::as_u64).filter(|&v| v <= 15).map(|v| v as u8),
                    rate_of_turn: body.get("RateOfTurn").and_then(Value::as_i64).filter(|&v| v != -128).map(|v| v as i32),
                },
                None,
            ))
        }
        "ExtendedClassBPositionReport" => {
            let (lat, lon) = position(body, meta)?;
            let info = StaticInfo {
                name: clean_text(body.get("Name")),
                ship_type: ship_type(body.get("Type")),
                length_beam: dimensions(body.get("Dimension")),
                ..StaticInfo::default()
            };
            Some(Update::Position(
                PositionUpdate {
                    mmsi,
                    lat,
                    lon,
                    name: info.name.clone().or(meta_name),
                    course: course(body.get("Cog")),
                    speed: speed(body.get("Sog")),
                    heading: heading(body.get("TrueHeading")),
                    nav_status: None,
                    rate_of_turn: None,
                },
                Some(info),
            ))
        }
        "ShipStaticData" => Some(Update::Static(
            mmsi,
            StaticInfo {
                name: clean_text(body.get("Name")).or(meta_name),
                ship_type: ship_type(body.get("Type")),
                imo: body.get("ImoNumber").and_then(Value::as_u64).filter(|&v| (1_000_000..=9_999_999).contains(&v)),
                callsign: clean_text(body.get("CallSign")),
                destination: clean_text(body.get("Destination")),
                eta: eta(body.get("Eta")),
                draught: body.get("MaximumStaticDraught").and_then(Value::as_f64).filter(|&v| v > 0.0 && v < 25.5),
                length_beam: dimensions(body.get("Dimension")),
            },
        )),
        "StaticDataReport" => {
            let mut info = StaticInfo::default();
            if let Some(a) = body.get("ReportA").filter(|r| valid(r)) {
                info.name = clean_text(a.get("Name"));
            }
            if let Some(b) = body.get("ReportB").filter(|r| valid(r)) {
                info.ship_type = ship_type(b.get("ShipType"));
                info.callsign = clean_text(b.get("CallSign"));
                info.length_beam = dimensions(b.get("Dimension"));
            }
            if info == StaticInfo::default() {
                return None;
            }
            Some(Update::Static(mmsi, info))
        }
        "StandardSearchAndRescueAircraftReport" => {
            let (lat, lon) = position(body, meta)?;
            Some(Update::SarAircraft {
                mmsi,
                lat,
                lon,
                altitude: body.get("Altitude").and_then(Value::as_f64).filter(|&v| v < 4095.0),
                speed: body.get("Sog").and_then(Value::as_f64).filter(|&v| v < 1023.0),
                course: course(body.get("Cog")),
            })
        }
        "AidsToNavigationReport" => {
            let (lat, lon) = position(body, meta)?;
            let mut name = clean_text(body.get("Name")).unwrap_or_default();
            if let Some(ext) = clean_text(body.get("NameExtension")) {
                name.push_str(&ext);
            }
            let flag = |a: &str, b: &str| body.get(a).or_else(|| body.get(b)).and_then(Value::as_bool).unwrap_or(false);
            Some(Update::AtoN(AtoN {
                mmsi,
                lat,
                lon,
                name,
                aton_type: body.get("Type").and_then(Value::as_u64).unwrap_or(0) as u32,
                virtual_aton: flag("VirtualAtoN", "VirtualAid"),
                off_position: flag("OffPosition", "OffPositionIndicator"),
                timestamp: chrono::Utc::now().timestamp(),
            }))
        }
        _ => None,
    }
}

fn valid(report: &Value) -> bool {
    report.get("Valid").and_then(Value::as_bool).unwrap_or(true)
}

/// Position from the message body, falling back to the stream metadata.
/// AIS uses 91/181 for "not available"; 0/0 is almost always a receiver fault.
fn position(body: &Value, meta: &Value) -> Option<(f64, f64)> {
    let lat = body.get("Latitude").and_then(Value::as_f64).or_else(|| meta.get("latitude").and_then(Value::as_f64))?;
    let lon = body.get("Longitude").and_then(Value::as_f64).or_else(|| meta.get("longitude").and_then(Value::as_f64))?;
    let usable = lat.is_finite() && lon.is_finite() && lat.abs() <= 90.0 && lon.abs() <= 180.0 && !(lat == 0.0 && lon == 0.0);
    usable.then_some((lat, lon))
}

fn course(v: Option<&Value>) -> Option<f64> {
    v.and_then(Value::as_f64).filter(|&c| (0.0..360.0).contains(&c))
}

fn heading(v: Option<&Value>) -> Option<f64> {
    v.and_then(Value::as_f64).filter(|&h| (0.0..360.0).contains(&h))
}

fn speed(v: Option<&Value>) -> Option<f64> {
    v.and_then(Value::as_f64).filter(|&s| (0.0..102.2).contains(&s))
}

fn ship_type(v: Option<&Value>) -> Option<u32> {
    v.and_then(Value::as_u64).filter(|&t| (1..=99).contains(&t)).map(|t| t as u32)
}

/// AIS text is padded with `@` and spaces.
fn clean_text(v: Option<&Value>) -> Option<String> {
    let s = v?.as_str()?.trim_matches(|c: char| c == '@' || c.is_whitespace() || c == '\0');
    let s = s.split('@').next().unwrap_or("").trim();
    (!s.is_empty()).then(|| s.to_string())
}

fn dimensions(v: Option<&Value>) -> Option<(u32, u32)> {
    let dim = v?;
    let get = |k: &str| dim.get(k).and_then(Value::as_u64).unwrap_or(0) as u32;
    let length = get("A") + get("B");
    let beam = get("C") + get("D");
    (length > 0).then_some((length, beam))
}

/// `MM-DD HH:MM` (UTC). Unavailable parts use AIS sentinel values.
fn eta(v: Option<&Value>) -> Option<String> {
    let e = v?;
    let get = |k: &str| e.get(k).and_then(Value::as_u64);
    let (month, day) = (get("Month")?, get("Day")?);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    match (get("Hour"), get("Minute")) {
        (Some(h), Some(m)) if h < 24 && m < 60 => Some(format!("{month:02}-{day:02} {h:02}:{m:02}")),
        _ => Some(format!("{month:02}-{day:02}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(kind: &str, body: Value) -> String {
        serde_json::json!({
            "MessageType": kind,
            "MetaData": { "MMSI": 244_123_456u64, "ShipName": "NORDIC STAR@@@@  ", "latitude": 53.5, "longitude": 9.9 },
            "Message": { kind: body },
        })
        .to_string()
    }

    #[test]
    fn position_reports_map_sentinels_to_none() {
        let text = msg(
            "PositionReport",
            serde_json::json!({ "Cog": 360.0, "Sog": 102.3, "TrueHeading": 511, "RateOfTurn": -128, "NavigationalStatus": 5, "Latitude": 53.54, "Longitude": 9.98 }),
        );
        let Some(Update::Position(p, None)) = parse_message(&text) else { panic!("expected position") };
        assert_eq!((p.lat, p.lon), (53.54, 9.98));
        assert_eq!(p.name.as_deref(), Some("NORDIC STAR"));
        assert_eq!((p.course, p.speed, p.heading, p.rate_of_turn), (None, None, None, None));
        assert_eq!(p.nav_status, Some(5));
    }

    #[test]
    fn unusable_positions_are_dropped() {
        for (lat, lon) in [(91.0, 181.0), (0.0, 0.0)] {
            let text = serde_json::json!({
                "MessageType": "PositionReport",
                "MetaData": { "MMSI": 1u64, "latitude": lat, "longitude": lon },
                "Message": { "PositionReport": { "Latitude": lat, "Longitude": lon } },
            })
            .to_string();
            assert!(parse_message(&text).is_none(), "{lat},{lon}");
        }
    }

    #[test]
    fn static_data_is_not_a_position() {
        let text = msg(
            "ShipStaticData",
            serde_json::json!({
                "Name": "NORDIC STAR", "Type": 70, "ImoNumber": 9_300_000, "CallSign": "ABCD ",
                "Destination": "HAMBURG@@", "MaximumStaticDraught": 11.5,
                "Eta": { "Month": 9, "Day": 24, "Hour": 24, "Minute": 60 },
                "Dimension": { "A": 150, "B": 30, "C": 15, "D": 15 },
            }),
        );
        let Some(Update::Static(mmsi, info)) = parse_message(&text) else { panic!("expected static") };
        assert_eq!(mmsi, 244_123_456);
        assert_eq!(info.ship_type, Some(70));
        assert_eq!(info.imo, Some(9_300_000));
        assert_eq!(info.destination.as_deref(), Some("HAMBURG"));
        assert_eq!(info.eta.as_deref(), Some("09-24"));
        assert_eq!(info.length_beam, Some((180, 30)));
    }

    #[test]
    fn class_b_static_parts_are_merged() {
        let text = msg(
            "StaticDataReport",
            serde_json::json!({ "ReportA": { "Valid": false, "Name": "IGNORED" }, "ReportB": { "Valid": true, "ShipType": 37, "CallSign": "DK1" } }),
        );
        let Some(Update::Static(_, info)) = parse_message(&text) else { panic!() };
        assert_eq!(info.name, None);
        assert_eq!(info.ship_type, Some(37));
    }

    #[test]
    fn aton_and_errors_are_recognised() {
        let text = msg(
            "AidsToNavigationReport",
            serde_json::json!({ "Name": "ELBE 1", "Type": 20, "VirtualAtoN": true, "Latitude": 54.0, "Longitude": 8.1 }),
        );
        let Some(Update::AtoN(a)) = parse_message(&text) else { panic!() };
        assert!(a.virtual_aton && !a.off_position);
        assert_eq!(a.name, "ELBE 1");
        assert_eq!(parse_message(r#"{"error":"Api Key Is Not Valid"}"#), Some(Update::Error("Api Key Is Not Valid".into())));
        assert!(parse_message("not json").is_none());
        assert!(parse_message(&msg("BaseStationReport", serde_json::json!({ "Latitude": 1.0, "Longitude": 1.0 }))).is_none());
    }
}
