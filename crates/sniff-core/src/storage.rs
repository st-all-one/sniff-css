//! Persistent session state: cookies + `localStorage` in a Playwright
//! `storageState`-style JSON format.
//!
//! The engine restores this state into a fresh browser session **before**
//! navigation (cookies via `Network.setCookies`, `localStorage` via an init
//! script that runs before the page's own scripts) and can re-export it after
//! a login performed through `actions`, so a session survives across server
//! restarts and browser relaunches.

use crate::error::SniffError;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

/// A single HTTP cookie.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    /// `domain`/`path` scope the cookie; at least one of them must be set to
    /// restore it via CDP (the cookie's `url` is derived from the domain).
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    /// Expiry as Unix seconds (TimeSinceEpoch). `None`/negative = session
    /// cookie (kept in memory, never written to disk).
    #[serde(default)]
    pub expires: Option<f64>,
    #[serde(default)]
    pub http_only: bool,
    #[serde(default)]
    pub secure: bool,
    /// One of `Strict` | `Lax` | `None` (CDP casing).
    #[serde(default)]
    pub same_site: Option<String>,
}

/// A single `localStorage` key/value pair for one origin.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KeyValue {
    pub name: String,
    pub value: String,
}

/// The `localStorage` contents of one origin.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OriginState {
    /// e.g. `http://localhost:10011`.
    pub origin: String,
    /// JSON key is `localStorage` for Playwright `storageState` compatibility.
    #[serde(default, rename = "localStorage")]
    pub local_storage: Vec<KeyValue>,
}

/// A persisted session: cookies + per-origin `localStorage`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct StorageState {
    #[serde(default)]
    pub cookies: Vec<Cookie>,
    #[serde(default)]
    pub origins: Vec<OriginState>,
}

impl StorageState {
    /// Load a state file (Playwright `storageState` JSON).
    pub fn from_file(path: &str) -> Result<Self, SniffError> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| SniffError::Other(format!("cannot read storage state `{path}`: {e}")))?;
        serde_json::from_str(&text)
            .map_err(|e| SniffError::Other(format!("invalid storage state `{path}`: {e}")))
    }

    /// Persist the state to `path` (atomic: temp file + rename).
    pub fn write_file(&self, path: &str) -> Result<(), SniffError> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| SniffError::Serialization(e.to_string()))?;
        let tmp = format!("{path}.tmp{}", std::process::id());
        std::fs::write(&tmp, &json)
            .map_err(|e| SniffError::Other(format!("cannot write storage state `{path}`: {e}")))?;
        std::fs::rename(&tmp, path).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            SniffError::Other(format!("cannot finalize storage state `{path}`: {e}"))
        })
    }

    /// Build the `cookies` array for CDP `Network.setCookies`.
    ///
    /// Cookies without a scope (`domain`/`path`/`url`) cannot be restored and
    /// are skipped; a leading `.` on the domain is normalized to a host-only
    /// set (CDP accepts both, but host-only matches the export shape).
    pub fn to_cdp_cookies(&self) -> Vec<Value> {
        self.cookies
            .iter()
            .filter_map(|c| {
                if c.domain.is_none() && c.path.is_none() {
                    return None;
                }
                let mut map = Map::new();
                map.insert("name".into(), json!(c.name));
                map.insert("value".into(), json!(c.value));
                if let Some(domain) = &c.domain {
                    let domain = domain.strip_prefix('.').unwrap_or(domain).to_string();
                    map.insert("domain".into(), json!(domain));
                }
                if let Some(path) = &c.path {
                    map.insert("path".into(), json!(path));
                }
                if let Some(expires) = c.expires.filter(|e| *e > 0.0) {
                    map.insert("expires".into(), json!(expires));
                }
                map.insert("httpOnly".into(), json!(c.http_only));
                map.insert("secure".into(), json!(c.secure));
                if let Some(same_site) = c.same_site.as_deref()
                    && (same_site == "Strict" || same_site == "Lax" || same_site == "None")
                {
                    map.insert("sameSite".into(), json!(same_site));
                }
                Some(Value::Object(map))
            })
            .collect()
    }

    /// Build a single init script that restores each origin's `localStorage`
    /// before the page's own scripts run. The script is origin-guarded and
    /// never throws (frames on `about:blank` or opaque origins have no
    /// `localStorage`). `None` when there is nothing to restore.
    pub fn to_init_script(&self) -> Option<String> {
        let origins = self
            .origins
            .iter()
            .filter(|o| !o.local_storage.is_empty())
            .collect::<Vec<_>>();
        if origins.is_empty() {
            return None;
        }
        let entries = origins
            .iter()
            .map(|o| {
                let origin = o.origin.clone();
                let items = o
                    .local_storage
                    .iter()
                    .map(|kv| (kv.name.clone(), kv.value.clone()))
                    .collect::<Vec<_>>();
                json!({ "origin": origin, "items": items })
            })
            .collect::<Vec<_>>();
        let payload = serde_json::to_string(&entries).unwrap_or_default();
        Some(format!(
            r#"(() => {{
  try {{
    const entries = {payload};
    for (const {{ origin, items }} of entries) {{
      if (location.origin !== origin) continue;
      for (const [name, value] of items) localStorage.setItem(name, value);
    }}
  }} catch (e) {{}}
}})()"#
        ))
    }

    /// Extract a state from a raw `Network.getAllCookies` result and the
    /// current page's `location.origin`/`localStorage` (both returned by
    /// `Runtime.evaluate`). Cookies with no value/name are dropped.
    pub fn from_cdp(raw_cookies: &[Value], page_state: Option<Value>) -> Self {
        let cookies = raw_cookies
            .iter()
            .filter_map(|c| {
                let name = c.get("name")?.as_str()?.to_string();
                let value = c.get("value")?.as_str()?.to_string();
                let expires = match c.get("expires").and_then(Value::as_f64) {
                    Some(e) if e > 0.0 => Some(e),
                    _ => None,
                };
                Some(Cookie {
                    name,
                    value,
                    domain: c.get("domain").and_then(Value::as_str).map(str::to_string),
                    path: c.get("path").and_then(Value::as_str).map(str::to_string),
                    expires,
                    http_only: c.get("httpOnly").and_then(Value::as_bool).unwrap_or(false),
                    secure: c.get("secure").and_then(Value::as_bool).unwrap_or(false),
                    same_site: c
                        .get("sameSite")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                })
            })
            .collect::<Vec<_>>();

        let mut origins = Vec::new();
        if let Some(state) = page_state
            && let (Some(origin), Some(items)) = (
                state.get("origin").and_then(Value::as_str),
                state.get("items").and_then(Value::as_array),
            )
        {
            let items = items
                .iter()
                .filter_map(|kv| {
                    Some(KeyValue {
                        name: kv.get("name")?.as_str()?.to_string(),
                        value: kv.get("value")?.as_str()?.to_string(),
                    })
                })
                .collect::<Vec<_>>();
            if !items.is_empty() {
                origins.push(OriginState {
                    origin: origin.to_string(),
                    local_storage: items,
                });
            }
        }

        Self { cookies, origins }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> StorageState {
        StorageState {
            cookies: vec![Cookie {
                name: "laravel_session".into(),
                value: "abc".into(),
                domain: Some("localhost".into()),
                path: Some("/".into()),
                expires: None,
                http_only: true,
                secure: false,
                same_site: Some("Lax".into()),
            }],
            origins: vec![OriginState {
                origin: "http://localhost:10011".into(),
                local_storage: vec![KeyValue {
                    name: "theme".into(),
                    value: "dark".into(),
                }],
            }],
        }
    }

    #[test]
    fn cdp_cookies_skip_session_expiry_and_normalize_domain() {
        let cookies = sample().to_cdp_cookies();
        assert_eq!(cookies.len(), 1);
        let c = &cookies[0];
        assert_eq!(c["domain"], "localhost");
        assert!(!c.as_object().unwrap().contains_key("expires"));
        assert_eq!(c["httpOnly"], true);
        assert_eq!(c["sameSite"], "Lax");
    }

    #[test]
    fn cdp_cookies_drop_unscoped() {
        let state = StorageState {
            cookies: vec![Cookie {
                name: "orphan".into(),
                value: "x".into(),
                domain: None,
                path: None,
                expires: None,
                http_only: false,
                secure: false,
                same_site: None,
            }],
            origins: vec![],
        };
        assert!(state.to_cdp_cookies().is_empty());
    }

    #[test]
    fn cdp_cookies_keep_expiring() {
        let mut state = sample();
        state.cookies[0].expires = Some(1_800_000_000.0);
        let c = &state.to_cdp_cookies()[0];
        assert_eq!(c["expires"], 1_800_000_000.0);
    }

    #[test]
    fn init_script_is_origin_guarded() {
        let script = sample().to_init_script().unwrap();
        assert!(script.contains("location.origin !== origin"));
        assert!(script.contains("localStorage.setItem"));
        assert!(script.contains("http://localhost:10011"));
    }

    #[test]
    fn init_script_none_when_no_localstorage() {
        let state = StorageState::default();
        assert!(state.to_init_script().is_none());
    }

    #[test]
    fn round_trip_via_json() {
        let state = sample();
        let json = serde_json::to_string(&state).unwrap();
        let back: StorageState = serde_json::from_str(&json).unwrap();
        assert_eq!(state, back);
    }

    #[test]
    fn from_cdp_extracts_cookies_and_page_state() {
        let raw = serde_json::from_str::<Value>(
            r#"[{"name":"a","value":"b","domain":".localhost","path":"/",
                "expires":-1,"httpOnly":true,"secure":false,"sameSite":"Lax"}]"#,
        )
        .unwrap();
        let raw = raw.as_array().unwrap().clone();
        let page = json!({"origin": "http://localhost:10011",
                          "items": [{"name":"k","value":"v"}]});
        let state = StorageState::from_cdp(&raw, Some(page));
        assert_eq!(state.cookies.len(), 1);
        assert_eq!(state.cookies[0].domain.as_deref(), Some(".localhost"));
        assert_eq!(state.cookies[0].expires, None);
        assert_eq!(state.origins.len(), 1);
        assert_eq!(state.origins[0].local_storage[0].value, "v");
    }
}
