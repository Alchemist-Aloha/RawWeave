//! Workflow presentation preferences, separate from graph documents/history.
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub struct Workspace {
    pub library_width: f32,
    pub viewer_width: f32,
    pub inspector_height: f32,
    pub library_open: bool,
    pub viewer_open: bool,
    pub inspector_open: bool,
}
impl Default for Workspace {
    fn default() -> Self {
        Self {
            library_width: 220.0,
            viewer_width: 380.0,
            inspector_height: 220.0,
            library_open: true,
            viewer_open: true,
            inspector_open: true,
        }
    }
}
impl Workspace {
    pub fn from_json(text: &str) -> Result<Self, String> {
        let doc: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        if !doc.is_object()
            || doc
                .get("version")
                .is_some_and(|version| version.as_u64() != Some(1))
        {
            return Err("Unsupported workspace preferences".into());
        }
        let defaults = Self::default();
        let size = |key: &str, fallback: f32, min: f64, max: f64| {
            doc.get(key)
                .and_then(Value::as_f64)
                .filter(|n| n.is_finite())
                .map_or(fallback, |n| n.clamp(min, max) as f32)
        };
        let open = |key: &str, fallback| doc.get(key).and_then(Value::as_bool).unwrap_or(fallback);
        Ok(Self {
            library_width: size("library_width", defaults.library_width, 180.0, 400.0),
            viewer_width: size("viewer_width", defaults.viewer_width, 280.0, 720.0),
            inspector_height: size("inspector_height", defaults.inspector_height, 100.0, 600.0),
            library_open: open("library_open", true),
            viewer_open: open("viewer_open", true),
            inspector_open: open("inspector_open", true),
        })
    }
    pub fn to_json(&self) -> String {
        serde_json::json!({"version":1, "library_width":self.library_width, "viewer_width":self.viewer_width,
            "inspector_height":self.inspector_height, "library_open":self.library_open, "viewer_open":self.viewer_open,
            "inspector_open":self.inspector_open}).to_string()
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        match std::fs::metadata(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error.to_string()),
            Ok(_) => {
                let bytes = crate::bounded_read(path, 64 * 1024)?;
                Self::from_json(std::str::from_utf8(&bytes).map_err(|e| e.to_string())?)
            }
        }
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        crate::save_workflow_atomic(path, &self.to_json())
    }
}

pub fn preferences_path() -> Option<PathBuf> {
    let root = if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
    }?;
    root.is_absolute()
        .then(|| root.join("rawweave/gpui-workspace.json"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn presentation_preferences_roundtrip_and_validate_untrusted_sizes() {
        let prefs = Workspace {
            library_width: 260.0,
            viewer_width: 420.0,
            inspector_height: 280.0,
            library_open: false,
            inspector_open: false,
            ..Default::default()
        };
        assert_eq!(Workspace::from_json(&prefs.to_json()).unwrap(), prefs);
        assert_eq!(Workspace::from_json("{}").unwrap(), Workspace::default());
        let bounded = Workspace::from_json(r#"{"library_width":-1,"viewer_width":1e100,"inspector_height":0,"inspector_open":"false"}"#).unwrap();
        assert_eq!(
            (
                bounded.library_width,
                bounded.viewer_width,
                bounded.inspector_height
            ),
            (180.0, 720.0, 100.0)
        );
        assert!(bounded.inspector_open);
        assert!(Workspace::from_json("[]").is_err());
        assert!(Workspace::from_json("{bad}").is_err());
        assert!(Workspace::from_json(r#"{"version":2}"#).is_err());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config/workspace.json");
        prefs.save(&path).unwrap();
        assert_eq!(
            Workspace::from_json(&std::fs::read_to_string(path).unwrap()).unwrap(),
            prefs
        );
    }
}
