//! Scene JSON (docs/topology.md section 2), parsed leniently: missing fields, nulls,
//! numbers-as-strings and unknown levels all degrade to sensible defaults instead of failing.

use serde::{Deserialize, Deserializer};
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Level {
    P1,
    P2,
    P3,
    P4,
    Ok,
    #[default]
    Unknown,
}

impl Level {
    pub fn parse(s: &str) -> Level {
        match s.trim().to_ascii_lowercase().as_str() {
            "p1" => Level::P1,
            "p2" => Level::P2,
            "p3" => Level::P3,
            "p4" => Level::P4,
            "ok" => Level::Ok,
            _ => Level::Unknown,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Level::P1 => "p1",
            Level::P2 => "p2",
            Level::P3 => "p3",
            Level::P4 => "p4",
            Level::Ok => "ok",
            Level::Unknown => "unknown",
        }
    }
    /// LED colour from the contract palette.
    pub fn rgb(self) -> [f32; 3] {
        fn hex(h: u32) -> [f32; 3] {
            [((h >> 16) & 255) as f32 / 255.0, ((h >> 8) & 255) as f32 / 255.0, (h & 255) as f32 / 255.0]
        }
        match self {
            Level::P1 => hex(0xff4d5e),
            Level::P2 => hex(0xff8a3d),
            Level::P3 => hex(0xffd166),
            Level::P4 => hex(0x7da7d9),
            Level::Ok => hex(0x3ddc97),
            Level::Unknown => hex(0x5b6478),
        }
    }
}

fn de_string<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::String(s) => s,
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => String::new(),
    })
}

fn de_f64<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::Number(n) => n.as_f64().unwrap_or(0.0),
        Value::String(s) => s.trim().parse().unwrap_or(0.0),
        Value::Bool(b) => b as u8 as f64,
        _ => 0.0,
    })
}

fn de_bool<'de, D: Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::Bool(b) => b,
        Value::Number(n) => n.as_f64().unwrap_or(0.0) != 0.0,
        Value::String(s) => s == "true",
        _ => false,
    })
}

fn de_level<'de, D: Deserializer<'de>>(d: D) -> Result<Level, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::String(s) => Level::parse(&s),
        _ => Level::Unknown,
    })
}

/// Deserialize a list, skipping elements that fail to parse (and treating null as empty).
fn de_list<'de, D: Deserializer<'de>, T: for<'a> Deserialize<'a>>(d: D) -> Result<Vec<T>, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::Array(items) => items.into_iter().filter_map(|v| T::deserialize(v).ok()).collect(),
        _ => Vec::new(),
    })
}

fn de_obj<'de, D: Deserializer<'de>, T: for<'a> Deserialize<'a> + Default>(d: D) -> Result<T, D::Error> {
    Ok(T::deserialize(Value::deserialize(d)?).unwrap_or_default())
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Open {
    #[serde(deserialize_with = "de_f64")]
    pub p1: f64,
    #[serde(deserialize_with = "de_f64")]
    pub p2: f64,
    #[serde(deserialize_with = "de_f64")]
    pub p3: f64,
    #[serde(deserialize_with = "de_f64")]
    pub p4: f64,
    #[serde(deserialize_with = "de_f64")]
    pub total: f64,
}

/// Open findings per kind (section 5; absent on older servers).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Kinds {
    #[serde(deserialize_with = "de_f64")]
    pub vuln: f64,
    #[serde(deserialize_with = "de_f64")]
    pub secret: f64,
    #[serde(deserialize_with = "de_f64")]
    pub misconfig: f64,
    #[serde(deserialize_with = "de_f64")]
    pub sast: f64,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Status {
    #[serde(deserialize_with = "de_level")]
    pub level: Level,
    #[serde(deserialize_with = "de_obj")]
    pub open: Open,
    #[serde(deserialize_with = "de_bool")]
    pub stale: bool,
    #[serde(deserialize_with = "de_obj")]
    pub kinds: Kinds,
}

impl Status {
    pub fn count(&self, l: Level) -> u32 {
        let v = match l {
            Level::P1 => self.open.p1,
            Level::P2 => self.open.p2,
            Level::P3 => self.open.p3,
            Level::P4 => self.open.p4,
            _ => 0.0,
        };
        v.max(0.0).round() as u32
    }
    pub fn secrets(&self) -> u32 {
        self.kinds.secret.max(0.0).round() as u32
    }
    pub fn misconfigs(&self) -> u32 {
        self.kinds.misconfig.max(0.0).round() as u32
    }
}

fn de_strings<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::Array(items) => items.into_iter().filter_map(|v| v.as_str().map(|s| s.to_string())).filter(|s| !s.is_empty()).collect(),
        _ => Vec::new(),
    })
}

/// An image asset in the inventory (`scene.images`).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct InventoryImage {
    #[serde(deserialize_with = "de_string")]
    pub id: String,
    #[serde(deserialize_with = "de_string")]
    pub name: String,
    #[serde(deserialize_with = "de_obj")]
    pub status: Status,
    #[serde(deserialize_with = "de_bool")]
    pub running: bool,
}

/// A scan source (`scene.sources`).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Source {
    #[serde(deserialize_with = "de_string")]
    pub name: String,
    #[serde(rename = "lastSeenAt", deserialize_with = "de_string")]
    pub last_seen_at: String,
    #[serde(deserialize_with = "de_bool")]
    pub stale: bool,
}

/// Days since 1970-01-01 (fractional) for an RFC 3339 timestamp; None when unparseable.
pub fn epoch_days(ts: &str) -> Option<f64> {
    let b = ts.as_bytes();
    if b.len() < 10 {
        return None;
    }
    let num = |s: &str| s.parse::<i64>().ok();
    let y = num(ts.get(0..4)?)?;
    let m = num(ts.get(5..7)?)?;
    let d = num(ts.get(8..10)?)?;
    let (hh, mm) = if b.len() >= 16 { (num(ts.get(11..13)?).unwrap_or(0), num(ts.get(14..16)?).unwrap_or(0)) } else { (0, 0) };
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    // Howard Hinnant's days_from_civil.
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    Some(days as f64 + (hh as f64 * 60.0 + mm as f64) / 1440.0)
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Image {
    #[serde(deserialize_with = "de_string")]
    pub id: String,
    #[serde(deserialize_with = "de_string")]
    pub name: String,
    #[serde(rename = "ref", deserialize_with = "de_string")]
    pub reference: String,
    #[serde(deserialize_with = "de_obj")]
    pub status: Status,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Workload {
    #[serde(deserialize_with = "de_string")]
    pub id: String,
    #[serde(deserialize_with = "de_string")]
    pub name: String,
    #[serde(deserialize_with = "de_string")]
    pub kind: String,
    #[serde(deserialize_with = "de_f64")]
    pub ready: f64,
    #[serde(deserialize_with = "de_f64")]
    pub desired: f64,
    #[serde(deserialize_with = "de_f64")]
    pub misconfig: f64,
    #[serde(deserialize_with = "de_obj")]
    pub status: Status,
    #[serde(deserialize_with = "de_list")]
    pub images: Vec<Image>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Group {
    #[serde(deserialize_with = "de_string")]
    pub id: String,
    #[serde(deserialize_with = "de_string")]
    pub name: String,
    #[serde(deserialize_with = "de_obj")]
    pub status: Status,
    #[serde(deserialize_with = "de_list")]
    pub workloads: Vec<Workload>,
    #[serde(deserialize_with = "de_strings")]
    pub domains: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Host {
    #[serde(deserialize_with = "de_string")]
    pub id: String,
    #[serde(deserialize_with = "de_string")]
    pub name: String,
    #[serde(deserialize_with = "de_string")]
    pub subtitle: String,
    #[serde(deserialize_with = "de_string")]
    pub cluster: String,
    #[serde(deserialize_with = "de_obj")]
    pub status: Status,
    #[serde(deserialize_with = "de_list")]
    pub groups: Vec<Group>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct External {
    #[serde(deserialize_with = "de_string")]
    pub id: String,
    #[serde(deserialize_with = "de_string")]
    pub kind: String,
    #[serde(deserialize_with = "de_string")]
    pub name: String,
    #[serde(deserialize_with = "de_obj")]
    pub status: Status,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Scene {
    #[serde(rename = "generatedAt", deserialize_with = "de_string")]
    pub generated_at: String,
    #[serde(deserialize_with = "de_list")]
    pub hosts: Vec<Host>,
    #[serde(deserialize_with = "de_list")]
    pub external: Vec<External>,
    #[serde(deserialize_with = "de_list")]
    pub images: Vec<InventoryImage>,
    #[serde(deserialize_with = "de_list")]
    pub sources: Vec<Source>,
}

impl Scene {
    /// Parse a scene. Only fails when the input is not JSON at all (or not an object).
    pub fn parse(json: &str) -> Result<Scene, String> {
        let v: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
        if !v.is_object() {
            return Err("scene is not a JSON object".into());
        }
        Scene::deserialize(v).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lenient_parse() {
        let s = Scene::parse(
            r#"{"hosts":[{"id":"h","name":null,"status":{"level":"weird","stale":"true"},
                 "groups":[{"id":"g","workloads":[{"id":"w","ready":"2","desired":null,"images":null}, 42]}]}, "nope"],
                 "external":null}"#,
        )
        .unwrap();
        assert_eq!(s.hosts.len(), 1);
        assert_eq!(s.hosts[0].status.level, Level::Unknown);
        assert!(s.hosts[0].status.stale);
        assert_eq!(s.hosts[0].groups[0].workloads.len(), 1);
        assert_eq!(s.hosts[0].groups[0].workloads[0].ready, 2.0);
        assert!(Scene::parse("[]").is_err());
        assert!(Scene::parse("{").is_err());
        assert_eq!(Scene::parse("{}").unwrap().hosts.len(), 0);
    }

    #[test]
    fn dates() {
        assert_eq!(epoch_days("1970-01-02T00:00:00Z"), Some(1.0));
        let a = epoch_days("2026-11-08T09:00:00Z").unwrap();
        let b = epoch_days("2026-10-30T09:00:00Z").unwrap();
        assert!((a - b - 9.0).abs() < 1e-9);
        assert_eq!(epoch_days("garbage"), None);
    }
}
