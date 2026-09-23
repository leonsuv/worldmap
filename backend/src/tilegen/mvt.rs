//! Mapbox Vector Tile 2.1 encoding (and decoding for inspection and tests).
//! <https://github.com/mapbox/vector-tile-spec/tree/master/2.1>

use std::collections::HashMap;
use std::sync::Arc;

pub const EXTENT: u32 = 4096;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    String(String),
    Int(i64),
    Double(f64),
    Bool(bool),
}

#[derive(Hash, PartialEq, Eq)]
enum ValueKey {
    S(String),
    I(i64),
    D(u64),
    B(bool),
}

impl Value {
    fn key(&self) -> ValueKey {
        match self {
            Value::String(s) => ValueKey::S(s.clone()),
            Value::Int(i) => ValueKey::I(*i),
            Value::Double(d) => ValueKey::D(d.to_bits()),
            Value::Bool(b) => ValueKey::B(*b),
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Value::String(_) => "String",
            Value::Int(_) | Value::Double(_) => "Number",
            Value::Bool(_) => "Boolean",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GeomType {
    Point = 1,
    LineString = 2,
    Polygon = 3,
}

pub type Properties = Arc<Vec<(Arc<str>, Value)>>;

/// One feature clipped to one tile, in tile coordinates.
#[derive(Clone, Debug)]
pub struct TileFeature {
    pub geom_type: GeomType,
    /// Parts (points, line strings or rings without closing point).
    pub parts: Vec<Vec<[i32; 2]>>,
    pub properties: Properties,
    /// Larger survives longer when a tile must be thinned.
    pub priority: f64,
}

// ── protobuf primitives ────────────────────────────────────────────────────

fn varint(buf: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        buf.push((v as u8) | 0x80);
        v >>= 7;
    }
    buf.push(v as u8);
}

fn tag(buf: &mut Vec<u8>, field: u32, wire: u32) {
    varint(buf, ((field << 3) | wire) as u64);
}

fn bytes_field(buf: &mut Vec<u8>, field: u32, data: &[u8]) {
    tag(buf, field, 2);
    varint(buf, data.len() as u64);
    buf.extend_from_slice(data);
}

fn packed(buf: &mut Vec<u8>, field: u32, values: &[u32]) {
    let mut inner = Vec::with_capacity(values.len() * 2);
    for &v in values {
        varint(&mut inner, v as u64);
    }
    bytes_field(buf, field, &inner);
}

fn zigzag(v: i32) -> u32 {
    ((v << 1) ^ (v >> 31)) as u32
}

fn command(id: u32, count: usize) -> u32 {
    (id & 0x7) | ((count as u32) << 3)
}

/// Encode parts as MVT geometry commands.
pub fn encode_geometry(geom_type: GeomType, parts: &[Vec<[i32; 2]>]) -> Vec<u32> {
    let mut out = Vec::new();
    let mut cursor = [0i32, 0i32];
    let mut delta = |out: &mut Vec<u32>, p: [i32; 2]| {
        out.push(zigzag(p[0] - cursor[0]));
        out.push(zigzag(p[1] - cursor[1]));
        cursor = p;
    };
    match geom_type {
        GeomType::Point => {
            let points: Vec<[i32; 2]> = parts.iter().flatten().copied().collect();
            if !points.is_empty() {
                out.push(command(1, points.len()));
                for p in points {
                    delta(&mut out, p);
                }
            }
        }
        GeomType::LineString | GeomType::Polygon => {
            for part in parts {
                let min = if geom_type == GeomType::Polygon { 3 } else { 2 };
                if part.len() < min {
                    continue;
                }
                out.push(command(1, 1));
                delta(&mut out, part[0]);
                out.push(command(2, part.len() - 1));
                for &p in &part[1..] {
                    delta(&mut out, p);
                }
                if geom_type == GeomType::Polygon {
                    out.push(command(7, 1));
                }
            }
        }
    }
    out
}

fn encode_value(v: &Value) -> Vec<u8> {
    let mut buf = Vec::new();
    match v {
        Value::String(s) => bytes_field(&mut buf, 1, s.as_bytes()),
        Value::Double(d) => {
            tag(&mut buf, 3, 1);
            buf.extend_from_slice(&d.to_le_bytes());
        }
        Value::Int(i) if *i >= 0 => {
            tag(&mut buf, 5, 0);
            varint(&mut buf, *i as u64);
        }
        Value::Int(i) => {
            tag(&mut buf, 6, 0);
            varint(&mut buf, ((*i << 1) ^ (*i >> 63)) as u64);
        }
        Value::Bool(b) => {
            tag(&mut buf, 7, 0);
            varint(&mut buf, *b as u64);
        }
    }
    buf
}

/// Encode one layer of features into a complete tile.
pub fn encode_tile(layers: &[(&str, &[TileFeature])]) -> Vec<u8> {
    let mut tile = Vec::new();
    for (name, features) in layers {
        let mut keys: Vec<Arc<str>> = Vec::new();
        let mut key_index: HashMap<Arc<str>, u32> = HashMap::new();
        let mut values: Vec<&Value> = Vec::new();
        let mut value_index: HashMap<ValueKey, u32> = HashMap::new();
        let mut body = Vec::new();
        tag(&mut body, 15, 0);
        varint(&mut body, 2);
        bytes_field(&mut body, 1, name.as_bytes());
        let mut encoded_any = false;
        for f in features.iter() {
            let geometry = encode_geometry(f.geom_type, &f.parts);
            if geometry.is_empty() {
                continue;
            }
            let mut tags = Vec::with_capacity(f.properties.len() * 2);
            for (k, v) in f.properties.iter() {
                let ki = *key_index.entry(k.clone()).or_insert_with(|| {
                    keys.push(k.clone());
                    keys.len() as u32 - 1
                });
                let vi = *value_index.entry(v.key()).or_insert_with(|| {
                    values.push(v);
                    values.len() as u32 - 1
                });
                tags.push(ki);
                tags.push(vi);
            }
            let mut feature = Vec::new();
            if !tags.is_empty() {
                packed(&mut feature, 2, &tags);
            }
            tag(&mut feature, 3, 0);
            varint(&mut feature, f.geom_type as u64);
            packed(&mut feature, 4, &geometry);
            bytes_field(&mut body, 2, &feature);
            encoded_any = true;
        }
        if !encoded_any {
            continue;
        }
        for k in &keys {
            bytes_field(&mut body, 3, k.as_bytes());
        }
        for v in &values {
            bytes_field(&mut body, 4, &encode_value(v));
        }
        tag(&mut body, 5, 0);
        varint(&mut body, EXTENT as u64);
        bytes_field(&mut tile, 3, &body);
    }
    tile
}

// ── decoding (inspection and tests) ────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct DecodedFeature {
    pub geom_type: u32,
    pub properties: Vec<(String, Value)>,
    pub parts: Vec<Vec<[i32; 2]>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DecodedLayer {
    pub name: String,
    pub extent: u32,
    pub features: Vec<DecodedFeature>,
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }
    fn done(&self) -> bool {
        self.pos >= self.data.len()
    }
    fn varint(&mut self) -> anyhow::Result<u64> {
        let mut v = 0u64;
        for shift in (0..64).step_by(7) {
            let b = *self.data.get(self.pos).ok_or_else(|| anyhow::anyhow!("truncated varint"))?;
            self.pos += 1;
            v |= ((b & 0x7f) as u64) << shift;
            if b < 0x80 {
                return Ok(v);
            }
        }
        anyhow::bail!("varint too long")
    }
    fn bytes(&mut self) -> anyhow::Result<&'a [u8]> {
        let len = self.varint()? as usize;
        let end = self.pos.checked_add(len).filter(|&e| e <= self.data.len()).ok_or_else(|| anyhow::anyhow!("truncated field"))?;
        let out = &self.data[self.pos..end];
        self.pos = end;
        Ok(out)
    }
    fn skip(&mut self, wire: u64) -> anyhow::Result<()> {
        match wire {
            0 => {
                self.varint()?;
            }
            1 => self.pos += 8,
            2 => {
                self.bytes()?;
            }
            5 => self.pos += 4,
            _ => anyhow::bail!("unsupported wire type {wire}"),
        }
        Ok(())
    }
    fn packed(&mut self) -> anyhow::Result<Vec<u32>> {
        let mut r = Reader::new(self.bytes()?);
        let mut out = Vec::new();
        while !r.done() {
            out.push(r.varint()? as u32);
        }
        Ok(out)
    }
}

fn decode_value(data: &[u8]) -> anyhow::Result<Value> {
    let mut r = Reader::new(data);
    let mut value = Value::Bool(false);
    while !r.done() {
        let key = r.varint()?;
        value = match (key >> 3, key & 7) {
            (1, 2) => Value::String(String::from_utf8(r.bytes()?.to_vec())?),
            (2, 5) => {
                let b: [u8; 4] = r.data[r.pos..r.pos + 4].try_into()?;
                r.pos += 4;
                Value::Double(f32::from_le_bytes(b) as f64)
            }
            (3, 1) => {
                let b: [u8; 8] = r.data[r.pos..r.pos + 8].try_into()?;
                r.pos += 8;
                Value::Double(f64::from_le_bytes(b))
            }
            (4, 0) | (5, 0) => Value::Int(r.varint()? as i64),
            (6, 0) => {
                let v = r.varint()?;
                Value::Int(((v >> 1) as i64) ^ -((v & 1) as i64))
            }
            (7, 0) => Value::Bool(r.varint()? != 0),
            (_, wire) => {
                r.skip(wire)?;
                continue;
            }
        };
    }
    Ok(value)
}

fn decode_geometry(commands: &[u32]) -> Vec<Vec<[i32; 2]>> {
    let mut parts: Vec<Vec<[i32; 2]>> = Vec::new();
    let (mut x, mut y) = (0i32, 0i32);
    let unzig = |v: u32| ((v >> 1) as i32) ^ -((v & 1) as i32);
    let mut i = 0;
    while i < commands.len() {
        let (id, count) = (commands[i] & 7, commands[i] >> 3);
        i += 1;
        match id {
            1 | 2 => {
                for _ in 0..count {
                    let (Some(&dx), Some(&dy)) = (commands.get(i), commands.get(i + 1)) else { return parts };
                    x += unzig(dx);
                    y += unzig(dy);
                    i += 2;
                    if id == 1 {
                        parts.push(vec![[x, y]]);
                    } else if let Some(last) = parts.last_mut() {
                        last.push([x, y]);
                    }
                }
            }
            _ => {}
        }
    }
    parts
}

/// Decode a (decompressed) tile.
pub fn decode_tile(data: &[u8]) -> anyhow::Result<Vec<DecodedLayer>> {
    let mut layers = Vec::new();
    let mut r = Reader::new(data);
    while !r.done() {
        let key = r.varint()?;
        if key >> 3 != 3 || key & 7 != 2 {
            r.skip(key & 7)?;
            continue;
        }
        let mut lr = Reader::new(r.bytes()?);
        let (mut name, mut extent) = (String::new(), 4096);
        let (mut keys, mut values, mut raw_features) = (Vec::new(), Vec::new(), Vec::new());
        while !lr.done() {
            let key = lr.varint()?;
            match (key >> 3, key & 7) {
                (1, 2) => name = String::from_utf8(lr.bytes()?.to_vec())?,
                (2, 2) => raw_features.push(lr.bytes()?),
                (3, 2) => keys.push(String::from_utf8(lr.bytes()?.to_vec())?),
                (4, 2) => values.push(decode_value(lr.bytes()?)?),
                (5, 0) => extent = lr.varint()? as u32,
                (_, wire) => lr.skip(wire)?,
            }
        }
        let mut features = Vec::new();
        for raw in raw_features {
            let mut fr = Reader::new(raw);
            let (mut geom_type, mut tags, mut geometry) = (0, Vec::new(), Vec::new());
            while !fr.done() {
                let key = fr.varint()?;
                match (key >> 3, key & 7) {
                    (2, 2) => tags = fr.packed()?,
                    (3, 0) => geom_type = fr.varint()? as u32,
                    (4, 2) => geometry = fr.packed()?,
                    (_, wire) => fr.skip(wire)?,
                }
            }
            let properties = tags
                .chunks(2)
                .filter_map(|c| Some((keys.get(*c.first()? as usize)?.clone(), values.get(*c.get(1)? as usize)?.clone())))
                .collect();
            features.push(DecodedFeature { geom_type, properties, parts: decode_geometry(&geometry) });
        }
        layers.push(DecodedLayer { name, extent, features });
    }
    Ok(layers)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn props(pairs: &[(&str, Value)]) -> Properties {
        Arc::new(pairs.iter().map(|(k, v)| (Arc::from(*k), v.clone())).collect())
    }

    #[test]
    fn spec_example_geometries_encode_exactly() {
        // Examples from the MVT 2.1 specification, section 4.3.5.
        assert_eq!(encode_geometry(GeomType::Point, &[vec![[25, 17]]]), vec![9, 50, 34]);
        assert_eq!(encode_geometry(GeomType::Point, &[vec![[5, 7]], vec![[3, 2]]]), vec![17, 10, 14, 3, 9]);
        assert_eq!(encode_geometry(GeomType::LineString, &[vec![[2, 2], [2, 10], [10, 10]]]), vec![9, 4, 4, 18, 0, 16, 16, 0]);
        assert_eq!(encode_geometry(GeomType::Polygon, &[vec![[3, 6], [8, 12], [20, 34]]]), vec![9, 6, 12, 18, 10, 12, 24, 44, 15]);
    }

    #[test]
    fn tiles_round_trip_through_the_decoder() {
        let features = vec![
            TileFeature {
                geom_type: GeomType::LineString,
                parts: vec![vec![[0, 0], [100, 100]], vec![[200, 0], [300, -50]]],
                properties: props(&[("voltage_kv", Value::Int(380)), ("name", Value::String("Nord".into())), ("hvdc", Value::Bool(false))]),
                priority: 1.0,
            },
            TileFeature {
                geom_type: GeomType::Point,
                parts: vec![vec![[4096, 4096]]],
                properties: props(&[("voltage_kv", Value::Int(380)), ("load", Value::Double(-1.5)), ("delta", Value::Int(-7))]),
                priority: 0.0,
            },
        ];
        let tile = encode_tile(&[("hvlines", &features)]);
        let layers = decode_tile(&tile).unwrap();
        assert_eq!(layers.len(), 1);
        let layer = &layers[0];
        assert_eq!((layer.name.as_str(), layer.extent), ("hvlines", 4096));
        assert_eq!(layer.features.len(), 2);
        assert_eq!(layer.features[0].parts, features[0].parts);
        assert_eq!(layer.features[0].properties[0], ("voltage_kv".into(), Value::Int(380)));
        assert_eq!(layer.features[1].properties[1], ("load".into(), Value::Double(-1.5)));
        assert_eq!(layer.features[1].properties[2], ("delta".into(), Value::Int(-7)));
        assert_eq!(layer.features[1].geom_type, 1);
    }

    #[test]
    fn empty_layers_are_omitted() {
        let degenerate =
            vec![TileFeature { geom_type: GeomType::LineString, parts: vec![vec![[1, 1]]], properties: props(&[]), priority: 0.0 }];
        assert!(encode_tile(&[("x", &degenerate)]).is_empty());
    }
}
