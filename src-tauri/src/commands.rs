//! WAID backend commands: read / write / list project briefs, fire webhooks.
//!
//! A "brief" is a single `.md` file with YAML frontmatter (metadata) and a
//! markdown body. We deliberately keep the format standard so the files remain
//! portable and openable in any editor — and, down the road, an Obsidian vault.
//!
//! TODO(obsidian): The briefs directory is just a flat folder today. To back
//! WAID with an Obsidian vault instead, swap the directory resolution in
//! `briefs_dir()` for the vault path (and respect Obsidian's `.obsidian/`
//! folder by skipping it in `list_briefs`). Nothing else in the format needs to
//! change — frontmatter + markdown is already Obsidian-native.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

/// A link button rendered in the detail pane (opens in the default browser).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Link {
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub url: String,
}

/// A webhook button rendered in the detail pane (fires an HTTP request).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Webhook {
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub url: String,
    /// HTTP method; defaults to POST when omitted in frontmatter.
    #[serde(default = "default_method")]
    pub method: String,
    /// Optional request body (sent as JSON for POST/PUT/PATCH).
    #[serde(default)]
    pub body: Option<String>,
}

fn default_method() -> String {
    "POST".to_string()
}

/// Frontmatter as parsed from YAML. Everything is optional so a half-written
/// brief still loads. Unknown keys are ignored (and preserved on disk because
/// edits round-trip through the raw file, not this struct).
#[derive(Debug, Clone, Deserialize, Default)]
struct FrontMatter {
    name: Option<String>,
    status: Option<String>,
    description: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    links: Vec<Link>,
    #[serde(default)]
    webhooks: Vec<Webhook>,
    last_opened: Option<String>,
}

/// A fully parsed brief, sent to the frontend. `raw` is the entire file (what
/// edit mode shows/saves); `body` is just the markdown after the frontmatter
/// (what the renderer displays).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Brief {
    pub path: String,
    pub file_name: String,
    pub name: String,
    pub status: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub links: Vec<Link>,
    pub webhooks: Vec<Webhook>,
    pub last_opened: Option<String>,
    pub body: String,
    pub raw: String,
}

/// Result of firing a webhook, returned to the frontend for toasts.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebhookResult {
    pub status: u16,
    pub ok: bool,
    pub body: String,
}

// --- Seed briefs ----------------------------------------------------------
// Bundled into the binary so the app has real content on first run regardless
// of working directory. These mirror the files in the repo's `briefs/` folder.
const SEED_BRIEFS: &[(&str, &str)] = &[
    ("gluefi.md", include_str!("../../briefs/gluefi.md")),
    ("blapp.md", include_str!("../../briefs/blapp.md")),
    ("solaris.md", include_str!("../../briefs/solaris.md")),
    (
        "the-thinking-room.md",
        include_str!("../../briefs/the-thinking-room.md"),
    ),
    ("whats-next.md", include_str!("../../briefs/whats-next.md")),
];

// --- Frontmatter parsing --------------------------------------------------

/// Split a file into its (optional) YAML frontmatter and markdown body.
/// Recognises a leading `---` line and a closing `---` line.
fn split_frontmatter(content: &str) -> (Option<String>, String) {
    let trimmed = content.strip_prefix('\u{feff}').unwrap_or(content);
    let without_lead = match trimmed.strip_prefix("---\n") {
        Some(rest) => rest,
        None => match trimmed.strip_prefix("---\r\n") {
            Some(rest) => rest,
            None => return (None, content.to_string()),
        },
    };

    // Find the closing delimiter line.
    let mut idx = 0usize;
    for line in without_lead.split_inclusive('\n') {
        let stripped = line.trim_end_matches(['\r', '\n']);
        if stripped == "---" {
            let yaml = &without_lead[..idx];
            let body_start = idx + line.len();
            let body = &without_lead[body_start..];
            return (Some(yaml.to_string()), body.trim_start_matches('\n').to_string());
        }
        idx += line.len();
    }

    // No closing delimiter — treat the whole thing as body.
    (None, content.to_string())
}

/// Parse a file's raw contents into a `Brief`.
fn parse_brief(path: &Path, raw: String) -> Brief {
    let (yaml, body) = split_frontmatter(&raw);

    let fm: FrontMatter = match &yaml {
        Some(y) => serde_yaml::from_str(y).unwrap_or_default(),
        None => FrontMatter::default(),
    };

    let file_name = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();

    // Fall back to a humanised filename when `name` is absent.
    let name = fm.name.filter(|s| !s.trim().is_empty()).unwrap_or_else(|| {
        path.file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| file_name.clone())
    });

    Brief {
        path: path.to_string_lossy().to_string(),
        file_name,
        name,
        status: fm.status,
        description: fm.description,
        tags: fm.tags,
        links: fm.links,
        webhooks: fm.webhooks,
        last_opened: fm.last_opened,
        body,
        raw,
    }
}

// --- Briefs directory resolution + settings -------------------------------

#[derive(Debug, Serialize, Deserialize, Default)]
struct Settings {
    briefs_dir: Option<String>,
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("could not resolve config dir: {e}"))?;
    Ok(dir.join("settings.json"))
}

fn load_settings(app: &AppHandle) -> Settings {
    match settings_path(app).ok().and_then(|p| fs::read_to_string(p).ok()) {
        Some(text) => serde_json::from_str(&text).unwrap_or_default(),
        None => Settings::default(),
    }
}

fn save_settings(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let path = settings_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(path, text).map_err(|e| e.to_string())
}

/// The default briefs location: `~/WAID/briefs`.
fn default_briefs_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let home = app
        .path()
        .home_dir()
        .map_err(|e| format!("could not resolve home dir: {e}"))?;
    Ok(home.join("WAID").join("briefs"))
}

/// Resolve the active briefs directory, creating + seeding it on first run.
fn briefs_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let settings = load_settings(app);
    let dir = match settings.briefs_dir {
        Some(ref s) if !s.trim().is_empty() => PathBuf::from(s),
        _ => default_briefs_dir(app)?,
    };

    let freshly_created = !dir.exists();
    if freshly_created {
        fs::create_dir_all(&dir).map_err(|e| format!("could not create briefs dir: {e}"))?;
    }

    // Seed sample briefs only when the directory is empty, so we never clobber
    // a user's real files.
    if dir_is_empty(&dir) {
        for (name, content) in SEED_BRIEFS {
            let target = dir.join(name);
            if !target.exists() {
                let _ = fs::write(&target, content);
            }
        }
    }

    Ok(dir)
}

fn dir_is_empty(dir: &Path) -> bool {
    match fs::read_dir(dir) {
        Ok(mut entries) => entries.next().is_none(),
        Err(_) => true,
    }
}

// --- Commands -------------------------------------------------------------

/// List every `.md` brief in the configured directory, parsed and sorted by
/// most-recently-opened (then name).
#[tauri::command]
pub fn list_briefs(app: AppHandle) -> Result<Vec<Brief>, String> {
    let dir = briefs_dir(&app)?;
    let mut briefs: Vec<Brief> = Vec::new();

    for entry in fs::read_dir(&dir).map_err(|e| format!("could not read briefs dir: {e}"))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        match fs::read_to_string(&path) {
            Ok(raw) => briefs.push(parse_brief(&path, raw)),
            Err(e) => eprintln!("WAID: skipping {}: {e}", path.display()),
        }
    }

    briefs.sort_by(|a, b| {
        b.last_opened
            .cmp(&a.last_opened)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    Ok(briefs)
}

/// Read and parse a single brief by absolute path.
#[tauri::command]
pub fn read_brief(path: String) -> Result<Brief, String> {
    let p = PathBuf::from(&path);
    let raw = fs::read_to_string(&p).map_err(|e| format!("could not read {path}: {e}"))?;
    Ok(parse_brief(&p, raw))
}

/// Overwrite a brief with new raw contents (frontmatter + body) and return the
/// reparsed result. This is the edit-mode save path — it round-trips the whole
/// file, so nothing in the frontmatter is lost or reordered by us.
#[tauri::command]
pub fn save_brief(path: String, content: String) -> Result<Brief, String> {
    let p = PathBuf::from(&path);
    fs::write(&p, &content).map_err(|e| format!("could not write {path}: {e}"))?;
    Ok(parse_brief(&p, content))
}

/// Update (or insert) the `last_opened` timestamp in a brief's frontmatter,
/// preserving all other keys. Returns the reparsed brief.
#[tauri::command]
pub fn touch_brief(path: String) -> Result<Brief, String> {
    let p = PathBuf::from(&path);
    let raw = fs::read_to_string(&p).map_err(|e| format!("could not read {path}: {e}"))?;
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

    let (yaml, body) = split_frontmatter(&raw);

    // Edit the frontmatter as a YAML mapping so every existing key survives.
    let mut map: serde_yaml::Mapping = match &yaml {
        Some(y) => serde_yaml::from_str(y).unwrap_or_default(),
        None => serde_yaml::Mapping::new(),
    };
    map.insert(
        serde_yaml::Value::from("last_opened"),
        serde_yaml::Value::from(now),
    );

    let yaml_out = serde_yaml::to_string(&map).map_err(|e| e.to_string())?;
    let new_raw = format!("---\n{}---\n\n{}", yaml_out, body);

    fs::write(&p, &new_raw).map_err(|e| format!("could not write {path}: {e}"))?;
    Ok(parse_brief(&p, new_raw))
}

/// Append a timestamped note under a `## Captures` heading in a brief's body.
/// Creates the heading if it doesn't exist yet. (Quick-capture path.)
#[tauri::command]
pub fn append_capture(path: String, note: String) -> Result<Brief, String> {
    let p = PathBuf::from(&path);
    let mut raw = fs::read_to_string(&p).map_err(|e| format!("could not read {path}: {e}"))?;
    let stamp = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();
    let entry = format!("- **{}** — {}", stamp, note.trim());

    if raw.contains("## Captures") {
        // Insert right after the Captures heading line.
        let mut out = String::with_capacity(raw.len() + entry.len() + 2);
        let mut inserted = false;
        for line in raw.split_inclusive('\n') {
            out.push_str(line);
            if !inserted && line.trim_end_matches(['\r', '\n']).trim() == "## Captures" {
                if !line.ends_with('\n') {
                    out.push('\n');
                }
                out.push_str(&entry);
                out.push('\n');
                inserted = true;
            }
        }
        raw = out;
    } else {
        if !raw.ends_with('\n') {
            raw.push('\n');
        }
        raw.push_str(&format!("\n## Captures\n\n{}\n", entry));
    }

    fs::write(&p, &raw).map_err(|e| format!("could not write {path}: {e}"))?;
    Ok(parse_brief(&p, raw))
}

/// Fire a webhook and return its status + body for a toast.
#[tauri::command]
pub async fn fire_webhook(
    url: String,
    method: Option<String>,
    body: Option<String>,
) -> Result<WebhookResult, String> {
    let method = method.unwrap_or_else(default_method).to_uppercase();
    let client = reqwest::Client::new();

    let mut req = match method.as_str() {
        "GET" => client.get(&url),
        "POST" => client.post(&url),
        "PUT" => client.put(&url),
        "PATCH" => client.patch(&url),
        "DELETE" => client.delete(&url),
        other => return Err(format!("unsupported HTTP method: {other}")),
    };

    if let Some(b) = body {
        if !b.trim().is_empty() {
            req = req
                .header("content-type", "application/json")
                .body(b);
        }
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();

    Ok(WebhookResult {
        status: status.as_u16(),
        ok: status.is_success(),
        body: text,
    })
}

/// Return the active briefs directory (resolving + seeding on first call).
#[tauri::command]
pub fn get_briefs_dir(app: AppHandle) -> Result<String, String> {
    Ok(briefs_dir(&app)?.to_string_lossy().to_string())
}

/// Point WAID at a different briefs directory and persist the choice.
#[tauri::command]
pub fn set_briefs_dir(app: AppHandle, dir: String) -> Result<String, String> {
    let path = PathBuf::from(&dir);
    if !path.exists() {
        fs::create_dir_all(&path).map_err(|e| format!("could not create {dir}: {e}"))?;
    }
    let mut settings = load_settings(&app);
    settings.briefs_dir = Some(dir);
    save_settings(&app, &settings)?;
    Ok(path.to_string_lossy().to_string())
}

/// Create a new, empty-ish brief from a name and return it. The filename is a
/// slugified version of the name.
#[tauri::command]
pub fn create_brief(app: AppHandle, name: String) -> Result<Brief, String> {
    let dir = briefs_dir(&app)?;
    let slug = slugify(&name);
    if slug.is_empty() {
        return Err("project name is empty".into());
    }
    let path = dir.join(format!("{slug}.md"));
    if path.exists() {
        return Err(format!("a brief named {slug}.md already exists"));
    }
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let content = format!(
        "---\nname: {name}\nstatus: active\ndescription: \ntags: []\nlinks: []\nwebhooks: []\nlast_opened: {now}\n---\n\n# {name}\n\nProject context goes here.\n"
    );
    fs::write(&path, &content).map_err(|e| format!("could not write {}: {e}", path.display()))?;
    Ok(parse_brief(&path, content))
}

fn slugify(name: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in name.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn splits_frontmatter_and_body() {
        let raw = "---\nname: Test\nstatus: active\n---\n\n# Body\n\nhello\n";
        let (yaml, body) = split_frontmatter(raw);
        assert!(yaml.unwrap().contains("name: Test"));
        assert!(body.starts_with("# Body"));
    }

    #[test]
    fn no_frontmatter_is_all_body() {
        let raw = "# Just markdown\n\nno frontmatter here";
        let (yaml, body) = split_frontmatter(raw);
        assert!(yaml.is_none());
        assert_eq!(body, raw);
    }

    #[test]
    fn parses_metadata_and_falls_back_to_filename() {
        let raw = "---\nstatus: active\ntags: [a, b]\nlinks:\n  - label: GH\n    url: https://x\n---\nbody";
        let brief = parse_brief(&PathBuf::from("/tmp/my-project.md"), raw.to_string());
        // No `name` in frontmatter -> humanised file stem.
        assert_eq!(brief.name, "my-project");
        assert_eq!(brief.status.as_deref(), Some("active"));
        assert_eq!(brief.tags, vec!["a", "b"]);
        assert_eq!(brief.links.len(), 1);
        assert_eq!(brief.links[0].label, "GH");
    }

    #[test]
    fn webhook_method_defaults_to_post() {
        let raw = "---\nwebhooks:\n  - label: deploy\n    url: https://x\n---\nbody";
        let brief = parse_brief(&PathBuf::from("/tmp/p.md"), raw.to_string());
        assert_eq!(brief.webhooks[0].method, "POST");
    }

    #[test]
    fn slugify_handles_punctuation_and_spaces() {
        assert_eq!(slugify("What's Next?"), "what-s-next");
        assert_eq!(slugify("  The Thinking Room  "), "the-thinking-room");
        assert_eq!(slugify("Gluefi"), "gluefi");
    }
}
