use crate::routes::ships::{AtoNReport, AtoNStore, SarAircraft, SarStore, ShipPosition, ShipStore};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio_tungstenite::tungstenite;

pub fn spawn_ais_fanout(
    api_key: String,
    tx: Arc<broadcast::Sender<String>>,
    ship_store: ShipStore,
    aton_store: AtoNStore,
    sar_store: SarStore,
) {
    tokio::spawn(async move {
        let mut backoff_secs = 1u64;

        loop {
            match connect_and_stream(&api_key, &tx, &ship_store, &aton_store, &sar_store).await {
                Ok(()) => {
                    tracing::info!("AIS stream closed normally, reconnecting...");
                    backoff_secs = 1;
                }
                Err(e) => {
                    tracing::error!("AIS stream error: {e}, reconnecting in {backoff_secs}s");
                }
            }

            tokio::time::sleep(std::time::Duration::from_secs(backoff_secs)).await;
            backoff_secs = (backoff_secs * 2).min(30);
        }
    });
}

async fn connect_and_stream(
    api_key: &str,
    tx: &broadcast::Sender<String>,
    ship_store: &ShipStore,
    aton_store: &AtoNStore,
    sar_store: &SarStore,
) -> anyhow::Result<()> {
    let url = "wss://stream.aisstream.io/v0/stream";
    let (ws_stream, _) = tokio_tungstenite::connect_async(url).await?;
    let (mut write, mut read) = ws_stream.split();

    // Subscribe to all message types that carry position or ship type info:
    //  - PositionReport:                 Class A position (msg 1/2/3)
    //  - StandardClassBPositionReport:   Class B position (msg 18)
    //  - ExtendedClassBPositionReport:   Class B position + ship type (msg 19)
    //  - ShipStaticData:                 Class A static data with Type (msg 5)
    //  - StaticDataReport:               Class B static data with ShipType (msg 24)
    let subscribe_msg = serde_json::json!({
        "APIKey": api_key,
        "BoundingBoxes": [[[-90, -180], [90, 180]]],
        "FilterMessageTypes": [
            "PositionReport",
            "StandardClassBPositionReport",
            "ExtendedClassBPositionReport",
            "ShipStaticData",
            "StaticDataReport",
            "AidsToNavigationReport",
            "StandardSearchAndRescueAircraftReport",
            "BaseStationReport",
            "LongRangeAisBroadcastMessage"
        ]
    });
    write
        .send(tungstenite::Message::Text(subscribe_msg.to_string().into()))
        .await?;

    tracing::info!("Connected to AIS stream, waiting for messages...");

    let mut msg_count: u64 = 0;

    while let Some(msg) = read.next().await {
        let msg = msg?;
        let text = match &msg {
            tungstenite::Message::Text(t) => t.as_str().to_string(),
            tungstenite::Message::Binary(b) => {
                match String::from_utf8(b.to_vec()) {
                    Ok(s) => s,
                    Err(_) => continue,
                }
            }
            tungstenite::Message::Close(frame) => {
                tracing::warn!("AIS stream close frame: {frame:?}");
                continue;
            }
            _ => continue,
        };

        msg_count += 1;
        if msg_count % 5000 == 0 {
            tracing::info!("AIS stream: {msg_count} messages, {} ships in store", ship_store.len());
        }

        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) {
            let msg_type = parsed.get("MessageType").and_then(|v| v.as_str());
            match msg_type {
                Some("PositionReport") => {
                    if let Some(feature) = handle_position_report(&parsed, ship_store) {
                        let _ = tx.send(feature);
                    }
                }
                Some("StandardClassBPositionReport") => {
                    if let Some(feature) = handle_classb_position(&parsed, ship_store) {
                        let _ = tx.send(feature);
                    }
                }
                Some("ExtendedClassBPositionReport") => {
                    if let Some(feature) = handle_extended_classb(&parsed, ship_store) {
                        let _ = tx.send(feature);
                    }
                }
                Some("ShipStaticData") => {
                    handle_ship_static_data(&parsed, ship_store);
                }
                Some("StaticDataReport") => {
                    handle_static_data_report(&parsed, ship_store);
                }
                Some("AidsToNavigationReport") => {
                    handle_aton_report(&parsed, aton_store);
                }
                Some("StandardSearchAndRescueAircraftReport") => {
                    if let Some(feature) = handle_sar_report(&parsed, sar_store) {
                        let _ = tx.send(feature);
                    }
                }
                Some("BaseStationReport") => {
                    // Base station reports (msg 4) — coastal MMSI, use as position update
                    if let Some(feature) = handle_base_station(&parsed, ship_store) {
                        let _ = tx.send(feature);
                    }
                }
                Some("LongRangeAisBroadcastMessage") => {
                    if let Some(feature) = handle_long_range(&parsed, ship_store) {
                        let _ = tx.send(feature);
                    }
                }
                _ => {}
            }
        }
    }

    Ok(())
}

// ─── Helpers ───

fn extract_meta(msg: &serde_json::Value) -> Option<(u64, f64, f64, String)> {
    let meta = msg.get("MetaData")?;
    let mmsi = meta.get("MMSI")?.as_u64()?;
    let lat = meta.get("latitude")?.as_f64()?;
    let lon = meta.get("longitude")?.as_f64()?;
    let ship_name = meta
        .get("ShipName")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    Some((mmsi, lat, lon, ship_name))
}

fn upsert_and_emit(
    ship_store: &ShipStore,
    mmsi: u64,
    lat: f64,
    lon: f64,
    ship_name: String,
    course: Option<f64>,
    speed: Option<f64>,
    heading: Option<f64>,
    new_type: Option<u32>,
    nav_status: Option<u8>,
    rate_of_turn: Option<i32>,
) -> String {
    // AIS special values: TrueHeading 511 = not available, COG 360.0 = not available
    let heading = heading.filter(|&h| h < 360.0);
    let course = course.filter(|&c| c < 360.0);

    // Merge with existing entry to preserve static data fields
    let existing = ship_store.get(&mmsi);
    let ship_type = new_type.or_else(|| existing.as_ref().and_then(|s| s.ship_type));
    let imo = existing.as_ref().and_then(|s| s.imo);
    let callsign = existing.as_ref().and_then(|s| s.callsign.clone());
    let destination = existing.as_ref().and_then(|s| s.destination.clone());
    let eta = existing.as_ref().and_then(|s| s.eta.clone());
    let draught = existing.as_ref().and_then(|s| s.draught);
    let length = existing.as_ref().and_then(|s| s.length);
    let beam = existing.as_ref().and_then(|s| s.beam);
    let nav_status = nav_status.or_else(|| existing.as_ref().and_then(|s| s.nav_status));
    let rate_of_turn = rate_of_turn.or_else(|| existing.as_ref().and_then(|s| s.rate_of_turn));

    ship_store.insert(mmsi, ShipPosition {
        mmsi,
        lat,
        lon,
        course,
        speed,
        heading,
        ship_name: ship_name.clone(),
        ship_type,
        timestamp: chrono::Utc::now().timestamp(),
        imo,
        callsign: callsign.clone(),
        destination: destination.clone(),
        eta: eta.clone(),
        draught,
        length,
        beam,
        nav_status,
        rate_of_turn,
    });

    serde_json::json!({
        "type": "Feature",
        "geometry": { "type": "Point", "coordinates": [lon, lat] },
        "properties": {
            "mmsi": mmsi,
            "ship_name": ship_name,
            "ship_type": ship_type,
            "course": course,
            "speed": speed,
            "heading": heading,
            "imo": imo,
            "callsign": callsign,
            "destination": destination,
            "eta": eta,
            "draught": draught,
            "length": length,
            "beam": beam,
            "nav_status": nav_status,
        }
    })
    .to_string()
}

fn set_ship_type(ship_store: &ShipStore, mmsi: u64, ship_type: u32, meta: &serde_json::Value) {
    if let Some(mut entry) = ship_store.get_mut(&mmsi) {
        entry.ship_type = Some(ship_type);
    } else {
        let ship_name = meta
            .get("ShipName")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let lat = meta.get("latitude").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let lon = meta.get("longitude").and_then(|v| v.as_f64()).unwrap_or(0.0);
        ship_store.insert(mmsi, ShipPosition {
            mmsi,
            lat,
            lon,
            course: None,
            speed: None,
            heading: None,
            ship_name,
            ship_type: Some(ship_type),
            timestamp: chrono::Utc::now().timestamp(),
            imo: None,
            callsign: None,
            destination: None,
            eta: None,
            draught: None,
            length: None,
            beam: None,
            nav_status: None,
            rate_of_turn: None,
        });
    }
}

/// Update static data fields from ShipStaticData (msg 5) or StaticDataReport (msg 24)
fn update_static_fields(ship_store: &ShipStore, mmsi: u64, static_msg: &serde_json::Value, meta: &serde_json::Value) {
    let imo = static_msg.get("ImoNumber").and_then(|v| v.as_u64()).filter(|&v| v > 0);
    let callsign = static_msg.get("CallSign").and_then(|v| v.as_str()).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let destination = static_msg.get("Destination").and_then(|v| v.as_str()).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let draught = static_msg.get("MaximumStaticDraught").and_then(|v| v.as_f64()).filter(|&v| v > 0.0);

    // ETA: {Month, Day, Hour, Minute}
    let eta = static_msg.get("Eta").and_then(|e| {
        let m = e.get("Month")?.as_u64()?;
        let d = e.get("Day")?.as_u64()?;
        let h = e.get("Hour")?.as_u64()?;
        let min = e.get("Minute")?.as_u64()?;
        if m == 0 && d == 0 { return None; }
        Some(format!("{:02}-{:02} {:02}:{:02}", m, d, h, min))
    });

    // Dimensions: A+B = length, C+D = beam
    let (length, beam) = static_msg.get("Dimension").map_or((None, None), |dim| {
        let a = dim.get("A").and_then(|v| v.as_u64()).unwrap_or(0);
        let b = dim.get("B").and_then(|v| v.as_u64()).unwrap_or(0);
        let c = dim.get("C").and_then(|v| v.as_u64()).unwrap_or(0);
        let d = dim.get("D").and_then(|v| v.as_u64()).unwrap_or(0);
        let l = (a + b) as u32;
        let w = (c + d) as u32;
        (if l > 0 { Some(l) } else { None }, if w > 0 { Some(w) } else { None })
    });

    if let Some(mut entry) = ship_store.get_mut(&mmsi) {
        if imo.is_some() { entry.imo = imo; }
        if callsign.is_some() { entry.callsign = callsign; }
        if destination.is_some() { entry.destination = destination; }
        if draught.is_some() { entry.draught = draught; }
        if eta.is_some() { entry.eta = eta; }
        if length.is_some() { entry.length = length; }
        if beam.is_some() { entry.beam = beam; }
    } else {
        let ship_name = meta.get("ShipName").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
        let lat = meta.get("latitude").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let lon = meta.get("longitude").and_then(|v| v.as_f64()).unwrap_or(0.0);
        ship_store.insert(mmsi, ShipPosition {
            mmsi,
            lat,
            lon,
            course: None,
            speed: None,
            heading: None,
            ship_name,
            ship_type: None,
            timestamp: chrono::Utc::now().timestamp(),
            imo,
            callsign,
            destination,
            eta,
            draught,
            length,
            beam,
            nav_status: None,
            rate_of_turn: None,
        });
    }
}

// ─── Message handlers ───

/// Class A position report (AIS msg 1/2/3) — most common, no ship type
fn handle_position_report(msg: &serde_json::Value, ship_store: &ShipStore) -> Option<String> {
    let (mmsi, lat, lon, ship_name) = extract_meta(msg)?;
    let pr = msg.get("Message")?.get("PositionReport")?;
    let course = pr.get("Cog").and_then(|v| v.as_f64());
    let speed = pr.get("Sog").and_then(|v| v.as_f64());
    let heading = pr.get("TrueHeading").and_then(|v| v.as_f64());
    let nav_status = pr.get("NavigationalStatus").and_then(|v| v.as_u64()).map(|v| v as u8);
    let rot = pr.get("RateOfTurn").and_then(|v| v.as_i64()).map(|v| v as i32);
    Some(upsert_and_emit(ship_store, mmsi, lat, lon, ship_name, course, speed, heading, None, nav_status, rot))
}

/// Class B standard position (AIS msg 18) — no ship type
fn handle_classb_position(msg: &serde_json::Value, ship_store: &ShipStore) -> Option<String> {
    let (mmsi, lat, lon, ship_name) = extract_meta(msg)?;
    let pr = msg.get("Message")?.get("StandardClassBPositionReport")?;
    let course = pr.get("Cog").and_then(|v| v.as_f64());
    let speed = pr.get("Sog").and_then(|v| v.as_f64());
    let heading = pr.get("TrueHeading").and_then(|v| v.as_f64());
    Some(upsert_and_emit(ship_store, mmsi, lat, lon, ship_name, course, speed, heading, None, None, None))
}

/// Class B extended position (AIS msg 19) — has position AND Type
fn handle_extended_classb(msg: &serde_json::Value, ship_store: &ShipStore) -> Option<String> {
    let (mmsi, lat, lon, ship_name) = extract_meta(msg)?;
    let pr = msg.get("Message")?.get("ExtendedClassBPositionReport")?;
    let course = pr.get("Cog").and_then(|v| v.as_f64());
    let speed = pr.get("Sog").and_then(|v| v.as_f64());
    let heading = pr.get("TrueHeading").and_then(|v| v.as_f64());
    let ship_type = pr.get("Type").and_then(|v| v.as_u64()).map(|v| v as u32);
    Some(upsert_and_emit(ship_store, mmsi, lat, lon, ship_name, course, speed, heading, ship_type, None, None))
}

/// Class A static data (AIS msg 5) — has Type, IMO, callsign, destination, dimensions
fn handle_ship_static_data(msg: &serde_json::Value, ship_store: &ShipStore) {
    let meta = match msg.get("MetaData") {
        Some(m) => m,
        None => return,
    };
    let mmsi = match meta.get("MMSI").and_then(|v| v.as_u64()) {
        Some(m) => m,
        None => return,
    };
    let static_msg = match msg.get("Message").and_then(|m| m.get("ShipStaticData")) {
        Some(s) => s,
        None => return,
    };
    if let Some(st) = static_msg.get("Type").and_then(|v| v.as_u64()) {
        set_ship_type(ship_store, mmsi, st as u32, meta);
    }
    update_static_fields(ship_store, mmsi, static_msg, meta);
}

/// Class B static data (AIS msg 24) — has ReportB.ShipType
fn handle_static_data_report(msg: &serde_json::Value, ship_store: &ShipStore) {
    let meta = match msg.get("MetaData") {
        Some(m) => m,
        None => return,
    };
    let mmsi = match meta.get("MMSI").and_then(|v| v.as_u64()) {
        Some(m) => m,
        None => return,
    };
    // ShipType is in ReportB (PartNumber=true)
    if let Some(st) = msg
        .get("Message")
        .and_then(|m| m.get("StaticDataReport"))
        .and_then(|s| s.get("ReportB"))
        .and_then(|b| b.get("ShipType"))
        .and_then(|v| v.as_u64())
    {
        if st > 0 {
            set_ship_type(ship_store, mmsi, st as u32, meta);
        }
    }
}

// ─── New message handlers ───

/// Aids to Navigation (AIS msg 21) — buoys, lighthouses, etc.
fn handle_aton_report(msg: &serde_json::Value, aton_store: &AtoNStore) {
    let meta = match msg.get("MetaData") {
        Some(m) => m,
        None => return,
    };
    let mmsi = match meta.get("MMSI").and_then(|v| v.as_u64()) {
        Some(m) => m,
        None => return,
    };
    let lat = meta.get("latitude").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let lon = meta.get("longitude").and_then(|v| v.as_f64()).unwrap_or(0.0);

    let aton = msg.get("Message").and_then(|m| m.get("AidsToNavigationReport"));
    let name = aton
        .and_then(|a| a.get("Name"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let aton_type = aton
        .and_then(|a| a.get("Type"))
        .and_then(|v| v.as_u64())
        .map(|v| v as u32)
        .unwrap_or(0);
    let virtual_aton = aton
        .and_then(|a| a.get("VirtualAid"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let off_position = aton
        .and_then(|a| a.get("OffPositionIndicator"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    aton_store.insert(mmsi, AtoNReport {
        mmsi,
        lat,
        lon,
        name,
        aton_type,
        virtual_aton,
        off_position,
        timestamp: chrono::Utc::now().timestamp(),
    });
}

/// Search and Rescue Aircraft (AIS msg 9)
fn handle_sar_report(msg: &serde_json::Value, sar_store: &SarStore) -> Option<String> {
    let meta = msg.get("MetaData")?;
    let mmsi = meta.get("MMSI")?.as_u64()?;
    let lat = meta.get("latitude")?.as_f64()?;
    let lon = meta.get("longitude")?.as_f64()?;
    let sar = msg.get("Message")?.get("StandardSearchAndRescueAircraftReport")?;
    let altitude = sar.get("Altitude").and_then(|v| v.as_f64());
    let speed = sar.get("Sog").and_then(|v| v.as_f64());
    let course = sar.get("Cog").and_then(|v| v.as_f64());

    sar_store.insert(mmsi, SarAircraft {
        mmsi,
        lat,
        lon,
        altitude,
        speed,
        course,
        timestamp: chrono::Utc::now().timestamp(),
    });

    Some(serde_json::json!({
        "type": "Feature",
        "geometry": { "type": "Point", "coordinates": [lon, lat] },
        "properties": {
            "mmsi": mmsi,
            "sar": true,
            "altitude": altitude,
            "speed": speed,
            "course": course,
        }
    }).to_string())
}

/// Base Station Report (AIS msg 4) — coastal stations / MMSI with position
fn handle_base_station(msg: &serde_json::Value, ship_store: &ShipStore) -> Option<String> {
    let (mmsi, lat, lon, ship_name) = extract_meta(msg)?;
    // Base stations don't move, just update position
    Some(upsert_and_emit(ship_store, mmsi, lat, lon, ship_name, None, None, None, None, None, None))
}

/// Long Range AIS Broadcast (AIS msg 27) — low-resolution, long-range position
fn handle_long_range(msg: &serde_json::Value, ship_store: &ShipStore) -> Option<String> {
    let (mmsi, lat, lon, ship_name) = extract_meta(msg)?;
    let lr = msg.get("Message")?.get("LongRangeAisBroadcastMessage")?;
    let course = lr.get("Cog").and_then(|v| v.as_f64());
    let speed = lr.get("Sog").and_then(|v| v.as_f64());
    let nav_status = lr.get("NavigationalStatus").and_then(|v| v.as_u64()).map(|v| v as u8);
    Some(upsert_and_emit(ship_store, mmsi, lat, lon, ship_name, course, speed, None, None, nav_status, None))
}
