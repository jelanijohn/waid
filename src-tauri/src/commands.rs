//! WAID backend commands: read / write / list project briefs, fire webhooks.
//!
//! A "brief" is a single `.md` file with YAML frontmatter (metadata) and a
//! markdown body. We deliberately keep the format standard so the files remain
//! portable and openable in any editor — and, down the road, an Obsidian vault.
//!
//! Obsidian: the briefs directory can be (a subfolder of) an Obsidian vault.
//! `list_briefs` walks subdirectories recursively and skips dot-entries, so
//! Obsidian's `.obsidian/` (and `.trash/`, `.git/`, …) never show up as briefs.
//! `get_vault_info` walks up to the nearest `.obsidian/` so the frontend can
//! offer "Open in Obsidian" deep links. The format needs nothing special —
//! frontmatter + markdown (incl. `[[wikilinks]]`) is already Obsidian-native.

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

/// A sync source for pulling external data *into* a brief (inverse of a
/// webhook). The primary case — a GitHub repo — is derived from `links` and
/// needs no frontmatter; this optional shape covers sources `links` can't
/// express (CI server, deploy-status endpoint, self-hosted JSON). Field
/// extraction is deliberately dumb: a flat `json_path -> label` map, no nested
/// templating. Serialized camelCase like the rest of the data model.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SyncSource {
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub url: String,
    /// HTTP method; defaults to GET (sources are reads).
    #[serde(default)]
    pub method: Option<String>,
    /// Optional flat extraction: `{ "data.count": "Open items" }`. Sorted keys
    /// (BTreeMap) so the rendered line is deterministic.
    #[serde(default)]
    pub fields: std::collections::BTreeMap<String, String>,
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
    /// Optional explicit sync sources (GitHub repos are derived from `links`).
    /// Defaults to empty so existing briefs deserialize unchanged.
    #[serde(default)]
    sources: Vec<SyncSource>,
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
    pub sources: Vec<SyncSource>,
    pub last_opened: Option<String>,
    /// When the managed sync block was last written (parsed back out of the
    /// body's `_synced …_` line — never stored in frontmatter). `null` if the
    /// brief has never been synced.
    pub last_synced: Option<String>,
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

/// One source's outcome within a sync. `line` is the rendered detail on success
/// (the text after the bold label) or an error message to show inline.
struct SourceResult {
    label: String,
    line: Result<String, String>,
}

/// Per-brief result of `sync_all`, returned to the frontend for an aggregate
/// toast. One failed brief never aborts the rest.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncOutcome {
    pub path: String,
    pub name: String,
    pub ok: bool,
    pub error: Option<String>,
}

// --- Seed briefs ----------------------------------------------------------
// Bundled into the binary so the app has real content on first run regardless
// of working directory. These mirror the files in the repo's `briefs/` folder.
const SEED_BRIEFS: &[(&str, &str)] = &[
    (
        "sample-web-app.md",
        include_str!("../../briefs/sample-web-app.md"),
    ),
    (
        "sample-mobile-app.md",
        include_str!("../../briefs/sample-mobile-app.md"),
    ),
    (
        "sample-side-project.md",
        include_str!("../../briefs/sample-side-project.md"),
    ),
    (
        "sample-research.md",
        include_str!("../../briefs/sample-research.md"),
    ),
    (
        "sample-archived.md",
        include_str!("../../briefs/sample-archived.md"),
    ),
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
        sources: fm.sources,
        last_opened: fm.last_opened,
        last_synced: extract_last_synced(&body),
        body,
        raw,
    }
}

// --- Brief sync: the managed block ----------------------------------------
//
// Synced output lives between two HTML-comment markers WAID owns. The markers
// are valid CommonMark (render to nothing in Obsidian), so the format stays
// portable. On each sync we replace everything between them; if they're absent
// we append the block at the end of the body. Everything *outside* the markers
// is sacred — body prose, frontmatter, key order, unknown keys are all
// preserved byte-for-byte (we only ever rewrite the body's marker region and
// re-attach the original frontmatter verbatim).

const SYNC_START: &str = "<!-- waid:sync:start -->";
const SYNC_END: &str = "<!-- waid:sync:end -->";

/// Split `raw` into `(prefix, body)` where `prefix + body == raw` byte-for-byte.
/// `prefix` is the frontmatter region (BOM + delimiters + everything up to and
/// including the closing `---` line); `body` is the markdown after it. Unlike
/// `split_frontmatter` this trims nothing, so re-joining is lossless — that's
/// what lets sync preserve the frontmatter exactly. No frontmatter (or no
/// closing delimiter) → `prefix` is empty and `body` is the whole file.
fn split_for_body_edit(raw: &str) -> (&str, &str) {
    let bom_len = if raw.starts_with('\u{feff}') {
        '\u{feff}'.len_utf8()
    } else {
        0
    };
    let after_bom = &raw[bom_len..];
    let (lead_len, rest) = if let Some(r) = after_bom.strip_prefix("---\n") {
        (4, r)
    } else if let Some(r) = after_bom.strip_prefix("---\r\n") {
        (5, r)
    } else {
        return ("", raw);
    };

    let mut idx = 0usize;
    for line in rest.split_inclusive('\n') {
        if line.trim_end_matches(['\r', '\n']) == "---" {
            let body_start = bom_len + lead_len + idx + line.len();
            return (&raw[..body_start], &raw[body_start..]);
        }
        idx += line.len();
    }
    // No closing delimiter — treat the whole file as body (matches
    // split_frontmatter's tolerance).
    ("", raw)
}

/// Replace the managed sync block in `body` with `rendered`, or append a fresh
/// block when no block exists yet. `Err` when the markers are malformed
/// (only one present, or end-before-start) — the caller leaves the file
/// untouched rather than risk eating user prose between a stray marker pair.
fn upsert_sync_block(body: &str, rendered: &str) -> Result<String, String> {
    let block = format!("{SYNC_START}\n{rendered}\n{SYNC_END}");
    let start = body.find(SYNC_START);
    let end = body.find(SYNC_END);

    match (start, end) {
        (Some(s), Some(e)) => {
            if e < s {
                return Err("malformed sync markers (end before start)".into());
            }
            let end_at = e + SYNC_END.len();
            let mut out = String::with_capacity(body.len() + block.len());
            out.push_str(&body[..s]);
            out.push_str(&block);
            out.push_str(&body[end_at..]);
            Ok(out)
        }
        (None, None) => {
            // Append, mirroring how append_capture grows the body: ensure a
            // blank line separates the block from any existing prose.
            let mut out = String::from(body);
            if !out.is_empty() {
                if !out.ends_with('\n') {
                    out.push('\n');
                }
                out.push('\n');
            }
            out.push_str(&block);
            out.push('\n');
            Ok(out)
        }
        _ => Err("malformed sync markers (only one of start/end present)".into()),
    }
}

/// Render the compact markdown that goes inside the managed block: one bold
/// line per source, then an italic `_synced …_` timestamp line. All formatting
/// lives here (in Rust) so the frontend just re-reads the brief. `synced_at` is
/// passed in (not read from the clock) so this stays pure and unit-testable.
fn render_sync_summary(results: &[SourceResult], synced_at: &str) -> String {
    let mut out = String::new();
    for r in results {
        match &r.line {
            Ok(detail) if detail.is_empty() => out.push_str(&format!("**{}**\n\n", r.label)),
            Ok(detail) => out.push_str(&format!("**{}** · {}\n\n", r.label, detail)),
            Err(e) => out.push_str(&format!("**{}** · ⚠️ {}\n\n", r.label, e)),
        }
    }
    out.push_str(&format!("_synced {synced_at}_"));
    out
}

/// Pull the `_synced …_` timestamp back out of a body's managed block, if any.
/// This is how `Brief.last_synced` is populated without ever touching
/// frontmatter.
fn extract_last_synced(body: &str) -> Option<String> {
    let start = body.find(SYNC_START)?;
    let end = body.find(SYNC_END)?;
    if end < start {
        return None;
    }
    for line in body[start..end].lines() {
        if let Some(rest) = line.trim().strip_prefix("_synced ") {
            let ts = rest.trim_end_matches('_').trim();
            if !ts.is_empty() {
                return Some(ts.to_string());
            }
        }
    }
    None
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

// --- Obsidian vault detection ---------------------------------------------

/// What the frontend needs to offer "Open in Obsidian". When the briefs dir is
/// (inside) a vault, `root` is the vault folder and `name` its basename — the
/// pieces of an `obsidian://open?vault=<name>&file=<relative path>` deep link.
#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct VaultInfo {
    pub is_vault: bool,
    pub name: Option<String>,
    pub root: Option<String>,
}

/// Find the nearest enclosing Obsidian vault by walking up from `start` looking
/// for an `.obsidian/` directory. Returns the vault root and its folder name.
fn resolve_vault(start: &Path) -> Option<(PathBuf, String)> {
    let mut current = Some(start);
    while let Some(dir) = current {
        if dir.join(".obsidian").is_dir() {
            let name = dir.file_name()?.to_string_lossy().to_string();
            return Some((dir.to_path_buf(), name));
        }
        current = dir.parent();
    }
    None
}

// --- Commands -------------------------------------------------------------

/// List every `.md` brief under the configured directory, parsed and sorted by
/// most-recently-opened (then name). Recurses into subfolders (Obsidian vaults
/// nest notes) while skipping dot-entries like `.obsidian/`, `.trash/`, `.git/`.
#[tauri::command]
pub fn list_briefs(app: AppHandle) -> Result<Vec<Brief>, String> {
    let dir = briefs_dir(&app)?;
    let mut briefs: Vec<Brief> = Vec::new();
    collect_briefs(&dir, &mut briefs)?;

    briefs.sort_by(|a, b| {
        b.last_opened
            .cmp(&a.last_opened)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    Ok(briefs)
}

/// Recursively gather `.md` briefs under `dir`, skipping hidden entries (any
/// file or folder whose name starts with `.`). Unreadable files are skipped
/// with a warning rather than failing the whole listing.
fn collect_briefs(dir: &Path, out: &mut Vec<Brief>) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|e| format!("could not read {}: {e}", dir.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        let path = entry.path();
        let file_type = entry.file_type().map_err(|e| e.to_string())?;
        if file_type.is_dir() {
            collect_briefs(&path, out)?;
        } else if path.extension().and_then(|s| s.to_str()) == Some("md") {
            match fs::read_to_string(&path) {
                Ok(raw) => out.push(parse_brief(&path, raw)),
                Err(e) => eprintln!("WAID: skipping {}: {e}", path.display()),
            }
        }
    }
    Ok(())
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

// --- Brief sync: fetching + the commands ----------------------------------
//
// Pull live data *into* a brief (inverse of fire_webhook). Sources are derived
// from the brief's own `links` (any github.com/{owner}/{repo}) plus any
// explicit `sources`. We fetch each, render a compact summary, and splice it
// into the managed block — frontmatter and prose untouched.

/// Parse `https://github.com/{owner}/{repo}` (tolerating extra path segments,
/// `www.`, http, and a trailing `.git`) into `(owner, repo)`. Non-GitHub URLs
/// (e.g. a Claude Project link) return `None`, so they stay navigational.
fn parse_github_url(url: &str) -> Option<(String, String)> {
    let rest = url
        .strip_prefix("https://github.com/")
        .or_else(|| url.strip_prefix("http://github.com/"))
        .or_else(|| url.strip_prefix("https://www.github.com/"))
        .or_else(|| url.strip_prefix("http://www.github.com/"))?;
    let mut segs = rest.split('/').filter(|s| !s.is_empty());
    let owner = segs.next()?.to_string();
    let repo = segs.next()?.trim_end_matches(".git").to_string();
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some((owner, repo))
}

/// Whether a brief has anything to sync (a GitHub link or an explicit source).
fn brief_is_syncable(brief: &Brief) -> bool {
    brief.sources.iter().any(|s| !s.url.trim().is_empty())
        || brief.links.iter().any(|l| parse_github_url(&l.url).is_some())
}

/// `"" `/`"s"` pluralization helper for counts.
fn plural(n: u64) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

/// Map a GitHub combined-status state to a compact glyph.
fn ci_symbol(state: &str) -> &'static str {
    match state {
        "success" => "✓",
        "failure" | "error" => "✗",
        "pending" => "…",
        _ => "?",
    }
}

/// Format an RFC3339 timestamp as a short relative string ("4h ago"). Falls
/// back to the raw string if it doesn't parse.
fn humanize_since(iso: &str) -> String {
    let then = match chrono::DateTime::parse_from_rfc3339(iso) {
        Ok(t) => t.with_timezone(&chrono::Utc),
        Err(_) => return iso.to_string(),
    };
    let secs = (chrono::Utc::now() - then).num_seconds();
    if secs < 60 {
        return "just now".to_string();
    }
    let mins = secs / 60;
    if mins < 60 {
        return format!("{mins}m ago");
    }
    let hours = mins / 60;
    if hours < 24 {
        return format!("{hours}h ago");
    }
    let days = hours / 24;
    if days < 7 {
        return format!("{days}d ago");
    }
    let weeks = days / 7;
    if weeks < 5 {
        return format!("{weeks}w ago");
    }
    let months = days / 30;
    if months < 12 {
        return format!("{months}mo ago");
    }
    format!("{}y ago", days / 365)
}

/// Build a `· `-joined GitHub detail line from the pieces we fetched. Any field
/// we couldn't read is simply omitted.
fn format_github_detail(
    open_prs: Option<u64>,
    open_issues: Option<u64>,
    pushed_at: Option<&str>,
    ci: Option<&str>,
    release: Option<&str>,
) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(p) = open_prs {
        parts.push(format!("{p} open PR{}", plural(p)));
    }
    if let Some(i) = open_issues {
        parts.push(format!("{i} open issue{}", plural(i)));
    }
    if let Some(p) = pushed_at {
        parts.push(format!("last push {}", humanize_since(p)));
    }
    if let Some(c) = ci {
        parts.push(format!("CI {}", ci_symbol(c)));
    }
    if let Some(r) = release {
        parts.push(format!("latest {r}"));
    }
    if parts.is_empty() {
        "no data".to_string()
    } else {
        parts.join(" · ")
    }
}

/// GET a GitHub API URL and parse the JSON body. Sends the required `User-Agent`
/// and adds `Authorization: Bearer <token>` when a token is stored in the OS
/// keyring (the keyring-backed auth seam — unauthenticated otherwise). Surfaces
/// rate-limiting (403/429 with `X-RateLimit-Remaining: 0`) as a clear message.
async fn gh_get_json(client: &reqwest::Client, url: &str) -> Result<serde_json::Value, String> {
    let mut req = client
        .get(url)
        .header(reqwest::header::USER_AGENT, "WAID")
        .header(reqwest::header::ACCEPT, "application/vnd.github+json");
    // keyring-backed auth: lifts the unauth rate limit and reaches private repos.
    if let Some(token) = github_token() {
        req = req.header(reqwest::header::AUTHORIZATION, format!("Bearer {token}"));
    }

    let resp = req.send().await.map_err(|e| format!("request failed: {e}"))?;
    let status = resp.status();
    if status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        let exhausted = resp
            .headers()
            .get("x-ratelimit-remaining")
            .and_then(|v| v.to_str().ok())
            == Some("0");
        if exhausted {
            return Err(
                "GitHub rate limit reached — add a token in Settings to raise it.".to_string(),
            );
        }
        return Err(format!("GitHub returned {status}"));
    }
    // 404 on a configured repo almost always means it's private and the token is
    // missing or under-scoped — GitHub hides private repos behind a 404 rather
    // than a 403. (Endpoints that 404 legitimately, e.g. releases/latest, have
    // their errors swallowed by the caller, so this hint only surfaces for the
    // required repo lookup.)
    if status == reqwest::StatusCode::NOT_FOUND {
        let hint = if github_token().is_some() {
            "if it's private, your token needs the `repo` scope (classic) or \
             Contents: Read access (fine-grained)"
        } else {
            "if it's private, add a token with repo access in Settings"
        };
        return Err(format!("not found — check the repo path; {hint}"));
    }
    if !status.is_success() {
        return Err(format!("GitHub returned {status}"));
    }
    resp.json::<serde_json::Value>()
        .await
        .map_err(|e| format!("invalid JSON from GitHub: {e}"))
}

/// Fetch a repo's headline state: open PR/issue counts, last push, default-branch
/// CI status, and latest release tag. CI uses the **combined commit status API**
/// (`/commits/{branch}/status` → single `state` field) rather than check-runs.
/// Only the repo endpoint is required; the rest degrade gracefully to omitted.
async fn fetch_github(
    client: &reqwest::Client,
    owner: &str,
    repo: &str,
) -> Result<String, String> {
    let base = "https://api.github.com";

    // Required: the repo itself (default branch, pushed_at, open issues+PRs).
    let repo_json = gh_get_json(client, &format!("{base}/repos/{owner}/{repo}")).await?;
    let pushed_at = repo_json
        .get("pushed_at")
        .and_then(|v| v.as_str())
        .map(String::from);
    let default_branch = repo_json
        .get("default_branch")
        .and_then(|v| v.as_str())
        .unwrap_or("main")
        .to_string();
    // GitHub's `open_issues_count` counts PRs too; we subtract the PR count.
    let open_issues_and_prs = repo_json
        .get("open_issues_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    // Open PR count via the search API (best-effort; tighter rate limit).
    let pr_url = format!(
        "{base}/search/issues?q=repo:{owner}/{repo}+type:pr+state:open&per_page=1"
    );
    let open_prs = gh_get_json(client, &pr_url)
        .await
        .ok()
        .and_then(|v| v.get("total_count").and_then(|c| c.as_u64()));
    let open_issues = open_prs.map(|p| open_issues_and_prs.saturating_sub(p));

    // Default-branch CI: only report when at least one status exists, so repos
    // with no CI don't show a misleading "pending".
    let status_url = format!("{base}/repos/{owner}/{repo}/commits/{default_branch}/status");
    let ci = gh_get_json(client, &status_url).await.ok().and_then(|v| {
        let total = v.get("total_count").and_then(|t| t.as_u64()).unwrap_or(0);
        if total == 0 {
            return None;
        }
        v.get("state").and_then(|s| s.as_str()).map(String::from)
    });

    // Latest release (404 when the repo has none → omitted).
    let release = gh_get_json(client, &format!("{base}/repos/{owner}/{repo}/releases/latest"))
        .await
        .ok()
        .and_then(|v| v.get("tag_name").and_then(|t| t.as_str()).map(String::from));

    Ok(format_github_detail(
        open_prs,
        open_issues,
        pushed_at.as_deref(),
        ci.as_deref(),
        release.as_deref(),
    ))
}

/// Walk a dotted `json_path` (numeric segments index into arrays).
fn json_path<'a>(value: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
    let mut cur = value;
    for seg in path.split('.') {
        cur = match seg.parse::<usize>() {
            Ok(idx) => cur.get(idx)?,
            Err(_) => cur.get(seg)?,
        };
    }
    Some(cur)
}

/// Render a JSON scalar compactly; non-scalars fall back to their JSON text.
fn value_to_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => "null".to_string(),
        other => other.to_string(),
    }
}

/// Fetch an explicit (non-GitHub) source and extract its configured fields into
/// a `label: value · …` detail line. Sources are reads, so only GET is issued.
async fn fetch_source(client: &reqwest::Client, src: &SyncSource) -> Result<String, String> {
    let json = gh_get_plain(client, &src.url).await?;
    if src.fields.is_empty() {
        return Ok(String::new());
    }
    let mut parts = Vec::with_capacity(src.fields.len());
    for (path, label) in &src.fields {
        let val = json_path(&json, path)
            .map(value_to_string)
            .unwrap_or_else(|| "—".to_string());
        parts.push(format!("{label}: {val}"));
    }
    Ok(parts.join(" · "))
}

/// GET an arbitrary URL and parse JSON (used by explicit sources). Sends a
/// `User-Agent` like the GitHub path but no auth header.
async fn gh_get_plain(client: &reqwest::Client, url: &str) -> Result<serde_json::Value, String> {
    let resp = client
        .get(url)
        .header(reqwest::header::USER_AGENT, "WAID")
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("returned {}", resp.status()));
    }
    resp.json::<serde_json::Value>()
        .await
        .map_err(|e| format!("invalid JSON: {e}"))
}

/// Gather every source result for a brief: GitHub links first, then explicit
/// `sources`. Per-source errors are collected (as `Err` lines) rather than
/// failing the whole sync. Returns `None` when the brief has no sources at all.
async fn collect_source_results(
    client: &reqwest::Client,
    brief: &Brief,
) -> Option<Vec<SourceResult>> {
    let mut results = Vec::new();
    for link in &brief.links {
        if let Some((owner, repo)) = parse_github_url(&link.url) {
            let label = format!("GitHub · {owner}/{repo}");
            let line = fetch_github(client, &owner, &repo).await;
            results.push(SourceResult { label, line });
        }
    }
    for src in &brief.sources {
        if src.url.trim().is_empty() {
            continue;
        }
        let label = if src.label.trim().is_empty() {
            src.url.clone()
        } else {
            src.label.clone()
        };
        let line = fetch_source(client, src).await;
        results.push(SourceResult { label, line });
    }
    if results.is_empty() {
        None
    } else {
        Some(results)
    }
}

/// Refresh a single brief's managed sync block from its linked integrations and
/// return the re-parsed brief. Reads + parses the brief, fetches each source,
/// renders the summary, and writes back through the whole-file round-trip
/// (frontmatter preserved byte-for-byte). Errors leave the file untouched.
#[tauri::command]
pub async fn sync_brief(path: String) -> Result<Brief, String> {
    let p = PathBuf::from(&path);
    let raw = fs::read_to_string(&p).map_err(|e| format!("could not read {path}: {e}"))?;
    let brief = parse_brief(&p, raw.clone());

    let client = reqwest::Client::new();
    let results = collect_source_results(&client, &brief)
        .await
        .ok_or_else(|| "No syncable sources (add a GitHub link or a `sources` entry).".to_string())?;

    let synced_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let rendered = render_sync_summary(&results, &synced_at);

    // Edit only the body's managed region; re-attach the original frontmatter
    // verbatim. upsert errors (malformed markers) abort before any write.
    let (prefix, body) = split_for_body_edit(&raw);
    let new_body = upsert_sync_block(body, &rendered)?;
    let new_raw = format!("{prefix}{new_body}");

    fs::write(&p, &new_raw).map_err(|e| format!("could not write {path}: {e}"))?;
    Ok(parse_brief(&p, new_raw))
}

/// Sync every brief that has a source, mapping `sync_brief` over the list.
/// Briefs with nothing to sync are skipped; one failure never aborts the rest.
#[tauri::command]
pub async fn sync_all(app: AppHandle) -> Result<Vec<SyncOutcome>, String> {
    let dir = briefs_dir(&app)?;
    let mut briefs: Vec<Brief> = Vec::new();
    collect_briefs(&dir, &mut briefs)?;

    let mut outcomes = Vec::new();
    for brief in briefs {
        if !brief_is_syncable(&brief) {
            continue;
        }
        let outcome = match sync_brief(brief.path.clone()).await {
            Ok(updated) => SyncOutcome {
                path: updated.path,
                name: updated.name,
                ok: true,
                error: None,
            },
            Err(e) => SyncOutcome {
                path: brief.path,
                name: brief.name,
                ok: false,
                error: Some(e),
            },
        };
        outcomes.push(outcome);
    }
    Ok(outcomes)
}

/// Return the active briefs directory (resolving + seeding on first call).
#[tauri::command]
pub fn get_briefs_dir(app: AppHandle) -> Result<String, String> {
    Ok(briefs_dir(&app)?.to_string_lossy().to_string())
}

/// Best-effort detection of a WSL environment. Used only to tailor the
/// "couldn't open URL" hint — WSL can't reach the Windows host without a URL
/// handler like `wslview` (from the `wslu` package).
#[tauri::command]
pub fn is_wsl() -> bool {
    if std::env::var_os("WSL_DISTRO_NAME").is_some() || std::env::var_os("WSL_INTEROP").is_some() {
        return true;
    }
    fs::read_to_string("/proc/sys/kernel/osrelease")
        .map(|s| {
            let s = s.to_lowercase();
            s.contains("microsoft") || s.contains("wsl")
        })
        .unwrap_or(false)
}

/// Report whether the active briefs directory sits inside an Obsidian vault,
/// and if so the vault's name + root path (used to build deep links).
#[tauri::command]
pub fn get_vault_info(app: AppHandle) -> Result<VaultInfo, String> {
    let dir = briefs_dir(&app)?;
    Ok(match resolve_vault(&dir) {
        Some((root, name)) => VaultInfo {
            is_vault: true,
            name: Some(name),
            root: Some(root.to_string_lossy().to_string()),
        },
        None => VaultInfo::default(),
    })
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

// --- Secrets (OS keyring) -------------------------------------------------
//
// Token storage for authenticated integrations (e.g. private GitHub repos).
// Secrets never touch `settings.json` or env vars — they live in the platform
// keychain (macOS Keychain, Windows Credential Manager, Linux Secret Service)
// via the `keyring` crate. On a headless / WSL box with no keyring daemon the
// store is simply unavailable; commands surface a clear error rather than
// silently falling back to plaintext.

/// Keyring "service" namespace for every WAID secret (the app identifier).
const KEYRING_SERVICE: &str = "com.jelanijohn.waid";

/// Well-known secret keys. The Brief Sync feature reads `GITHUB_TOKEN` to make
/// authenticated requests against private repos.
const SECRET_GITHUB_TOKEN: &str = "github.token";

/// Build a keyring entry for `key`, rejecting empty keys before touching the OS.
fn keyring_entry(key: &str) -> Result<keyring::Entry, String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("secret key is empty".into());
    }
    keyring::Entry::new(KEYRING_SERVICE, key).map_err(keyring_error)
}

/// Turn a keyring error into a user-facing message, with a Linux/WSL hint when
/// the platform store itself is missing (the common "no daemon running" case).
fn keyring_error(e: keyring::Error) -> String {
    match e {
        keyring::Error::NoStorageAccess(_) | keyring::Error::PlatformFailure(_) => format!(
            "OS keyring unavailable ({e}). On Linux/WSL a Secret Service daemon \
             (e.g. gnome-keyring) must be running to store secrets."
        ),
        other => other.to_string(),
    }
}

fn set_secret_value(key: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err("secret value is empty".into());
    }
    keyring_entry(key)?.set_password(value).map_err(keyring_error)
}

/// Read a secret; `Ok(None)` when no entry exists yet.
fn get_secret_value(key: &str) -> Result<Option<String>, String> {
    match keyring_entry(key)?.get_password() {
        Ok(v) => Ok(Some(v)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(keyring_error(e)),
    }
}

/// Delete a secret; succeeds even if it was already absent (idempotent).
fn delete_secret_value(key: &str) -> Result<(), String> {
    match keyring_entry(key)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(keyring_error(e)),
    }
}

/// Read the stored GitHub token, if any. The Brief Sync feature's GitHub
/// request builder (`gh_get_json`) calls this and adds an
/// `Authorization: Bearer <token>` header when a token is present.
pub(crate) fn github_token() -> Option<String> {
    get_secret_value(SECRET_GITHUB_TOKEN).ok().flatten()
}

/// Store (or replace) a secret in the OS keyring.
#[tauri::command]
pub fn set_secret(key: String, value: String) -> Result<(), String> {
    set_secret_value(&key, &value)
}

/// Read a secret from the OS keyring (`null` when not set).
#[tauri::command]
pub fn get_secret(key: String) -> Result<Option<String>, String> {
    get_secret_value(&key)
}

/// Remove a secret from the OS keyring.
#[tauri::command]
pub fn delete_secret(key: String) -> Result<(), String> {
    delete_secret_value(&key)
}

/// Report whether a secret is stored, without returning its value.
#[tauri::command]
pub fn has_secret(key: String) -> Result<bool, String> {
    Ok(get_secret_value(&key)?.is_some())
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
        assert_eq!(slugify("  Sample Research  "), "sample-research");
        assert_eq!(slugify("SampleApp"), "sampleapp");
    }

    /// Make a unique scratch directory under the system temp dir for a test.
    fn scratch_dir(tag: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!("waid-test-{}-{}", tag, std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        base
    }

    #[test]
    fn collect_briefs_recurses_and_skips_dot_dirs() {
        let root = scratch_dir("collect");
        fs::write(root.join("top.md"), "---\nname: Top\n---\nbody").unwrap();
        fs::create_dir_all(root.join("sub")).unwrap();
        fs::write(root.join("sub").join("nested.md"), "---\nname: Nested\n---\nbody").unwrap();
        // Things that must NOT be picked up.
        fs::create_dir_all(root.join(".obsidian")).unwrap();
        fs::write(root.join(".obsidian").join("app.md"), "should be skipped").unwrap();
        fs::write(root.join("notes.txt"), "not markdown").unwrap();

        let mut out = Vec::new();
        collect_briefs(&root, &mut out).unwrap();
        let mut names: Vec<_> = out.iter().map(|b| b.name.clone()).collect();
        names.sort();
        assert_eq!(names, vec!["Nested", "Top"]);

        fs::remove_dir_all(&root).unwrap();
    }

    /// Real end-to-end keyring roundtrip. Ignored by default because it needs a
    /// running Secret Service / keychain daemon. Run on demand with:
    ///   cargo test secret_real_roundtrip -- --ignored
    /// (On WSL, start + unlock gnome-keyring first — see README/setup notes.)
    #[test]
    #[ignore = "requires a running OS keyring daemon"]
    fn secret_real_roundtrip() {
        let key = "test.roundtrip";
        let _ = delete_secret_value(key); // clean slate
        assert_eq!(get_secret_value(key).unwrap(), None);
        set_secret_value(key, "hunter2").unwrap();
        assert_eq!(get_secret_value(key).unwrap().as_deref(), Some("hunter2"));
        set_secret_value(key, "rotated").unwrap(); // replace
        assert_eq!(get_secret_value(key).unwrap().as_deref(), Some("rotated"));
        delete_secret_value(key).unwrap();
        assert_eq!(get_secret_value(key).unwrap(), None);
        delete_secret_value(key).unwrap(); // idempotent
    }

    #[test]
    fn secret_validation_rejects_empty_key_and_value() {
        // These fail before any OS keyring access, so the test is hermetic.
        assert!(get_secret_value("").is_err());
        assert!(get_secret_value("   ").is_err());
        assert!(set_secret_value("k", "  ").is_err());
        assert!(set_secret_value("", "v").is_err());
    }

    // --- Brief sync helpers ------------------------------------------------

    fn ok_source(label: &str, detail: &str) -> SourceResult {
        SourceResult {
            label: label.to_string(),
            line: Ok(detail.to_string()),
        }
    }

    #[test]
    fn upsert_appends_block_to_empty_body() {
        let out = upsert_sync_block("", "hello").unwrap();
        assert_eq!(out, format!("{SYNC_START}\nhello\n{SYNC_END}\n"));
    }

    #[test]
    fn upsert_appends_block_after_prose() {
        let body = "# Title\n\nSome prose.\n";
        let out = upsert_sync_block(body, "DATA").unwrap();
        // Original prose is preserved verbatim at the front.
        assert!(out.starts_with("# Title\n\nSome prose.\n"));
        // A blank line separates prose from the appended block.
        assert!(out.contains(&format!("Some prose.\n\n{SYNC_START}\nDATA\n{SYNC_END}")));
    }

    #[test]
    fn upsert_replaces_existing_block_without_touching_surrounds() {
        let body = format!(
            "intro\n\n{SYNC_START}\nOLD\n{SYNC_END}\n\noutro prose\n"
        );
        let out = upsert_sync_block(&body, "NEW").unwrap();
        // Replaced content, single block, prose before AND after preserved.
        assert!(out.contains(&format!("{SYNC_START}\nNEW\n{SYNC_END}")));
        assert!(!out.contains("OLD"));
        assert!(out.starts_with("intro\n\n"));
        assert!(out.ends_with("\n\noutro prose\n"));
        assert_eq!(out.matches(SYNC_START).count(), 1);
    }

    #[test]
    fn upsert_rejects_malformed_markers() {
        // Only a start marker — refuse rather than risk eating prose.
        let just_start = format!("body\n{SYNC_START}\nstuff\n");
        assert!(upsert_sync_block(&just_start, "X").is_err());
        // Only an end marker.
        let just_end = format!("body\n{SYNC_END}\n");
        assert!(upsert_sync_block(&just_end, "X").is_err());
        // End before start.
        let reversed = format!("{SYNC_END}\nmid\n{SYNC_START}\n");
        assert!(upsert_sync_block(&reversed, "X").is_err());
    }

    #[test]
    fn render_sync_summary_formats_sources_and_timestamp() {
        let results = vec![
            ok_source("GitHub · a/b", "3 open PRs · 7 open issues"),
            SourceResult {
                label: "CI".to_string(),
                line: Err("returned 500".to_string()),
            },
        ];
        let out = render_sync_summary(&results, "2026-06-03T10:00:00Z");
        assert!(out.contains("**GitHub · a/b** · 3 open PRs · 7 open issues"));
        assert!(out.contains("**CI** · ⚠️ returned 500"));
        assert!(out.ends_with("_synced 2026-06-03T10:00:00Z_"));
    }

    #[test]
    fn extract_last_synced_round_trips_through_a_block() {
        let rendered = render_sync_summary(
            &[ok_source("GitHub · a/b", "1 open PR")],
            "2026-06-03T10:00:00Z",
        );
        let body = upsert_sync_block("# Brief\n", &rendered).unwrap();
        assert_eq!(
            extract_last_synced(&body).as_deref(),
            Some("2026-06-03T10:00:00Z")
        );
        // No block → no timestamp.
        assert_eq!(extract_last_synced("# Just prose"), None);
    }

    #[test]
    fn split_for_body_edit_is_lossless() {
        let raw = "---\nname: Test\nlinks: []\n---\n\n# Body\n\nprose\n";
        let (prefix, body) = split_for_body_edit(raw);
        assert_eq!(format!("{prefix}{body}"), raw); // byte-for-byte rejoin
        assert!(prefix.ends_with("---\n"));
        assert!(body.starts_with("\n# Body"));

        // No frontmatter → everything is body.
        let plain = "# No frontmatter\n";
        let (p2, b2) = split_for_body_edit(plain);
        assert_eq!(p2, "");
        assert_eq!(b2, plain);
    }

    #[test]
    fn sync_preserves_frontmatter_byte_for_byte() {
        // Simulate the sync write path (no network): the frontmatter prefix must
        // survive untouched while only the body's managed block changes.
        let raw = "---\nname: Test\nstatus: active\ncustom: keep-me\n---\n\n# Body\n\nprose\n";
        let (prefix, body) = split_for_body_edit(raw);
        let rendered = render_sync_summary(
            &[ok_source("GitHub · a/b", "ok")],
            "2026-06-03T10:00:00Z",
        );
        let new_body = upsert_sync_block(body, &rendered).unwrap();
        let new_raw = format!("{prefix}{new_body}");

        assert!(new_raw.starts_with(prefix)); // frontmatter unchanged
        assert!(new_raw.contains("custom: keep-me"));
        assert!(new_raw.contains("# Body\n\nprose"));
        assert!(new_raw.contains(SYNC_START));

        // Running it again replaces (does not duplicate) the block.
        let (p2, b2) = split_for_body_edit(&new_raw);
        let again = format!(
            "{p2}{}",
            upsert_sync_block(b2, &rendered).unwrap()
        );
        assert_eq!(again.matches(SYNC_START).count(), 1);
    }

    #[test]
    fn parse_github_url_extracts_owner_repo() {
        assert_eq!(
            parse_github_url("https://github.com/octocat/Hello-World"),
            Some(("octocat".to_string(), "Hello-World".to_string()))
        );
        // Extra path segments and a trailing .git are tolerated.
        assert_eq!(
            parse_github_url("https://github.com/octocat/Hello-World/issues"),
            Some(("octocat".to_string(), "Hello-World".to_string()))
        );
        assert_eq!(
            parse_github_url("https://github.com/octocat/Hello-World.git"),
            Some(("octocat".to_string(), "Hello-World".to_string()))
        );
        // Non-GitHub and incomplete URLs are not sources.
        assert_eq!(parse_github_url("https://claude.ai/project/abc"), None);
        assert_eq!(parse_github_url("https://github.com/octocat"), None);
    }

    #[test]
    fn json_path_walks_objects_and_arrays() {
        let v: serde_json::Value =
            serde_json::from_str(r#"{"data":{"items":[{"n":42}]}}"#).unwrap();
        assert_eq!(
            json_path(&v, "data.items.0.n").map(value_to_string),
            Some("42".to_string())
        );
        assert!(json_path(&v, "data.missing").is_none());
    }

    #[test]
    fn resolve_vault_walks_up_to_dot_obsidian() {
        let root = scratch_dir("vault");
        fs::create_dir_all(root.join(".obsidian")).unwrap();
        let briefs = root.join("Projects").join("briefs");
        fs::create_dir_all(&briefs).unwrap();

        let (found_root, name) = resolve_vault(&briefs).expect("should find the vault");
        assert_eq!(found_root, root);
        assert_eq!(name, root.file_name().unwrap().to_string_lossy());

        // A plain folder with no vault above it resolves to None.
        let plain = scratch_dir("plain");
        assert!(resolve_vault(&plain).is_none());

        fs::remove_dir_all(&root).unwrap();
        fs::remove_dir_all(&plain).unwrap();
    }
}
