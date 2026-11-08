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

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Status {
    #[serde(deserialize_with = "de_level")]
    pub level: Level,
    #[serde(deserialize_with = "de_obj")]
    pub open: Open,
    #[serde(deserialize_with = "de_bool")]
    pub stale: bool,
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
}
