//! `app.update.check`: asks for the newest REDA release and links to it when it is newer
//! than this build (docs/spec/v0.1/quality-and-release.md#packaging).
//!
//! REDA makes no network call on its own. The only one is this Command's, made when it
//! runs, through the [`ReleaseSource`] the app hands over with [`configure`]. `reda-ui`
//! itself has no network code: without a source the Command is unavailable, so tests and
//! headless engines never reach the network.

use std::sync::{Arc, Mutex, MutexGuard};

use reda_engine::Engine;
use reda_engine::command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Meta, NoParams, Unavailable,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Where REDA's releases are published: the GitHub Releases API's newest release (the
/// one marked Latest; the rolling `latest` release never is).
pub const LATEST_RELEASE_API: &str = "https://api.github.com/repos/openreda/reda/releases/latest";

/// A published release.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    /// Its version, without the tag's `v`, such as `0.1.4`.
    pub version: String,
    /// Its web page.
    pub url: String,
}

/// Fetches the newest release. Only [`app.update.check`](CheckForUpdates) calls it, once
/// per run.
pub trait ReleaseSource: Send + Sync {
    fn latest(&self) -> Result<Release, String>;
}

/// Reads a release from the GitHub Releases API's JSON (`tag_name`, `html_url`).
pub fn parse_github_release(json: &str) -> Result<Release, String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("the release is not JSON: {e}"))?;
    let field = |name: &str| {
        value
            .get(name)
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| format!("the release has no {name}"))
    };
    let tag = field("tag_name")?;
    let version = tag.strip_prefix('v').unwrap_or(tag);
    if Version::parse(version).is_none() {
        return Err(format!("the release tag {tag:?} is not a version"));
    }
    Ok(Release {
        version: version.to_owned(),
        url: field("html_url")?.to_owned(),
    })
}

/// Whether `latest` comes after `current` in semver order (build metadata such as
/// `+1a2b3c4` is ignored, and `0.2.0-dev` comes before `0.2.0`). Unreadable versions are
/// never newer.
pub fn is_newer(latest: &str, current: &str) -> bool {
    match (Version::parse(latest), Version::parse(current)) {
        (Some(latest), Some(current)) => latest > current,
        _ => false,
    }
}

/// A semver version's precedence: the numbers, then the pre-release identifiers.
#[derive(Debug, PartialEq, Eq)]
struct Version {
    numbers: [u64; 3],
    pre: Vec<Identifier>,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Identifier {
    // Numeric identifiers sort before alphanumeric ones.
    Number(u64),
    Text(String),
}

impl Version {
    fn parse(text: &str) -> Option<Version> {
        let text = text.split('+').next()?;
        let (core, pre) = match text.split_once('-') {
            Some((core, pre)) => (core, Some(pre)),
            None => (text, None),
        };
        let mut numbers = [0; 3];
        let mut parts = core.split('.');
        for n in &mut numbers {
            *n = parts.next()?.parse().ok()?;
        }
        if parts.next().is_some() {
            return None;
        }
        let pre = match pre {
            None => Vec::new(),
            Some(pre) => pre
                .split('.')
                .map(|id| match id.parse() {
                    Ok(n) => Some(Identifier::Number(n)),
                    Err(_) if !id.is_empty() => Some(Identifier::Text(id.to_owned())),
                    Err(_) => None,
                })
                .collect::<Option<_>>()?,
        };
        Some(Version { numbers, pre })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Version) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Version) -> std::cmp::Ordering {
        self.numbers.cmp(&other.numbers).then_with(|| {
            // A version without pre-release identifiers comes after one with them.
            match (self.pre.is_empty(), other.pre.is_empty()) {
                (true, true) => std::cmp::Ordering::Equal,
                (true, false) => std::cmp::Ordering::Greater,
                (false, true) => std::cmp::Ordering::Less,
                (false, false) => self.pre.cmp(&other.pre),
            }
        })
    }
}

/// What a check found.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct UpdateCheck {
    /// This build's version.
    pub current: String,
    /// The newest release's version.
    pub latest: String,
    /// Whether the newest release is newer than this build.
    pub newer: bool,
    /// The newest release's web page, when it is newer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// The app's version and release source, shared by the Command and the window, in the
/// engine's view state.
#[derive(Clone)]
pub struct Updates(Arc<Mutex<State>>);

struct State {
    version: String,
    source: Option<Arc<dyn ReleaseSource>>,
    /// Checks not yet shown by the window, oldest first.
    unseen: Vec<UpdateCheck>,
}

impl Default for Updates {
    fn default() -> Updates {
        Updates(Arc::new(Mutex::new(State {
            version: concat!(env!("CARGO_PKG_VERSION"), "-dev").to_owned(),
            source: None,
            unseen: Vec::new(),
        })))
    }
}

impl Updates {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The version this build reports.
    pub fn version(&self) -> String {
        self.lock().version.clone()
    }
}

/// The engine's [`Updates`], added to its view state on first use.
pub fn updates(engine: &mut Engine) -> Updates {
    if let Some(updates) = engine.view().get::<Updates>() {
        return updates.clone();
    }
    let updates = Updates::default();
    engine.view_mut().set(updates.clone());
    updates
}

/// Hands the app's version and release source to the engine. `reda-app` does this; until
/// then `app.update.check` is unavailable.
pub fn configure(engine: &mut Engine, version: &str, source: Option<Arc<dyn ReleaseSource>>) {
    let updates = updates(engine);
    let mut state = updates.lock();
    state.version = version.to_owned();
    state.source = source;
}

/// The version this build reports: `X.Y.Z` in a release, `X.Y.Z-dev+<git hash>` locally.
pub fn version(engine: &Engine) -> String {
    engine
        .view()
        .get::<Updates>()
        .map(Updates::version)
        .unwrap_or_else(|| Updates::default().version())
}

/// Takes the checks run since the last call, oldest first, for the window to show.
pub fn take_checks(engine: &Engine) -> Vec<UpdateCheck> {
    engine
        .view()
        .get::<Updates>()
        .map(|u| std::mem::take(&mut u.lock().unseen))
        .unwrap_or_default()
}

/// `app.update.check`.
pub(super) struct CheckForUpdates(pub(super) Updates);

impl Command for CheckForUpdates {
    const META: Meta = Meta {
        id: "app.update.check",
        title: "Check for Updates…",
        description: "Asks GitHub for the newest REDA release and links to it when it is newer than this build. REDA goes online only when this runs.",
        category: "app",
        icon: "update_check",
        kind: Kind::Query,
    };
    type Params = NoParams;
    type Output = UpdateCheck;

    fn available(&self, _: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        match self.0.lock().source {
            Some(_) => Ok(()),
            None => Err(Unavailable {
                reason: "requires the REDA app".into(),
            }),
        }
    }

    fn run(&self, _: &mut Context<'_>, _: NoParams) -> Result<UpdateCheck, CommandError> {
        let (current, source) = {
            let state = self.0.lock();
            let source = state
                .source
                .clone()
                .ok_or_else(|| CommandError::unavailable("requires the REDA app"))?;
            (state.version.clone(), source)
        };
        let release = source
            .latest()
            .map_err(|e| CommandError::unavailable(format!("could not check for updates: {e}")))?;
        let newer = is_newer(&release.version, &current);
        let check = UpdateCheck {
            current,
            latest: release.version,
            newer,
            url: newer.then_some(release.url),
        };
        self.0.lock().unseen.push(check.clone());
        Ok(check)
    }
}
