// SPDX-License-Identifier: MIT
//! Catalogs of templates of kinds (`docs/rfc/0005-pure-core.md`): the sources this device knows, read only when a
//! person asks, checked before anything of them is shown; and taking a template over into a space – the first time
//! or as an update – kind by kind, with a person's changes kept by a three-way merge.
//!
//! Fetching runs without the workspace ([`fetch_catalog`], [`fetch_template`]), so that a slow source never holds it;
//! what was fetched is then planned and applied in the workspace ([`Workspace::plan`], [`Workspace::apply`]).

use std::{collections::BTreeMap, io::Read, time::Duration};

use aktujo_base::Sha256Hash;
use aktujo_doc::{Conflict, Kind, TemplateRef, merge};
use serde::{Deserialize, Serialize};

use crate::{AppError, SpaceId, Workspace};

/// The official catalog, the source every device knows.
pub const DEFAULT_SOURCE: &str = "https://github.com/aktujo/catalog";
/// The author of the official templates.
pub const OFFICIAL_AUTHOR: &str = "aktujo";
/// The form of `catalog.json` this program reads.
pub const FORMAT: u32 = 1;
/// The setting with the sources of this device.
const SOURCES: &str = "catalog.sources";
/// The largest file read from a source.
const MAX_FILE: u64 = 8 * 1024 * 1024;

/// A source of templates: where a `catalog.json` lives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    /// Its address: a repository on GitHub, or any HTTPS address of a folder.
    pub url: String,
    /// Whether it is read.
    pub on: bool,
}

impl Source {
    /// Whether it is the official catalog – it can be switched off, not removed.
    pub fn is_default(&self) -> bool {
        self.url == DEFAULT_SOURCE
    }
}

/// What a source says about itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceInfo {
    /// Its name.
    pub name: String,
    /// What it offers.
    #[serde(default)]
    pub description: String,
    /// Where to read more.
    #[serde(default)]
    pub homepage: Option<String>,
}

/// A kind as a catalog lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListedKind {
    /// Its identity.
    pub id: String,
    /// Its name.
    pub name: String,
    /// Its name in other languages.
    #[serde(default)]
    pub names: BTreeMap<String, String>,
}

/// A template as a catalog lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// Its ID: `<author>.<name>`.
    pub id: String,
    /// Who made it.
    pub author: String,
    /// Where its `template.json` is, relative to the catalog.
    pub path: String,
    /// The SHA-256 of that file, in hexadecimal digits.
    pub sha256: String,
    /// Its licence.
    #[serde(default)]
    pub license: Option<String>,
    /// Its name.
    pub name: String,
    /// Its name in other languages.
    #[serde(default)]
    pub names: BTreeMap<String, String>,
    /// What it is for.
    #[serde(default)]
    pub description: String,
    /// That in other languages.
    #[serde(default)]
    pub descriptions: BTreeMap<String, String>,
    /// Its tags.
    #[serde(default)]
    pub tags: Vec<String>,
    /// The kinds in it.
    #[serde(default)]
    pub kinds: Vec<ListedKind>,
}

/// A `catalog.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Catalog {
    /// Its form.
    pub format: u32,
    /// The source.
    pub source: SourceInfo,
    /// The tags its templates use, with what they mean.
    #[serde(default)]
    pub tags: BTreeMap<String, String>,
    /// Its templates.
    #[serde(default)]
    pub templates: Vec<Entry>,
}

/// A template read and checked: its kinds and the hash of its file.
#[derive(Debug, Clone, PartialEq)]
pub struct Template {
    /// The kinds.
    pub kinds: Vec<Kind>,
    /// The SHA-256 of the file.
    pub sha256: Sha256Hash,
    /// Whether the file is a template (it names its author) rather than a list of kinds or an export of a space –
    /// only a template is remembered as where its kinds came from.
    pub is_template: bool,
}

/// Where a template comes from: a source and its ID there, or a file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Origin {
    /// The source's address; none for a file.
    pub source: Option<String>,
    /// The template's ID; none for a file.
    pub id: Option<String>,
}

fn catalog_error(message: impl Into<String>) -> AppError {
    AppError::Catalog(message.into())
}

/// The address a source is kept under: trimmed, without a slash at the end. Only HTTPS, or HTTP on this device.
pub fn source_url(url: &str) -> Result<String, AppError> {
    let url = url.trim().trim_end_matches('/');
    let local = ["http://localhost", "http://127.0.0.1", "http://[::1]"]
        .iter()
        .any(|l| url.strip_prefix(l).is_some_and(|rest| rest.is_empty() || rest.starts_with([':', '/'])));
    let host = url.strip_prefix("https://").map(|rest| rest.split('/').next().unwrap_or(""));
    if !(local || host.is_some_and(|h| !h.is_empty() && !h.contains(char::is_whitespace))) {
        return Err(catalog_error(format!("{url:?} is not an HTTPS address")));
    }
    Ok(url.to_owned())
}

/// Where the files of a source are: a repository on GitHub is read from its raw files (its default branch, or the
/// branch of `…/tree/<branch>`); any other address as it is.
fn files_of(url: &str) -> String {
    if let Some(rest) = url.strip_prefix("https://github.com/") {
        let parts: Vec<&str> = rest.split('/').collect();
        if let [owner, repository, rest @ ..] = parts.as_slice() {
            let branch = match rest {
                ["tree", branch @ ..] if !branch.is_empty() => branch.join("/"),
                _ => "HEAD".into(),
            };
            return format!("https://raw.githubusercontent.com/{owner}/{repository}/{branch}/");
        }
    }
    format!("{url}/")
}

fn get(address: &str) -> Result<Vec<u8>, AppError> {
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(30))).build().into();
    let mut response = agent.get(address).call().map_err(|e| catalog_error(format!("{address}: {e}")))?;
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(MAX_FILE + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| catalog_error(format!("{address}: {e}")))?;
    if bytes.len() as u64 > MAX_FILE {
        return Err(catalog_error(format!("{address}: larger than {MAX_FILE} bytes")));
    }
    Ok(bytes)
}

/// Reads the `catalog.json` of a source. A form newer than this program's is refused.
pub fn fetch_catalog(url: &str) -> Result<Catalog, AppError> {
    let url = source_url(url)?;
    let bytes = get(&format!("{}catalog.json", files_of(&url)))?;
    let format = serde_json::from_slice::<serde_json::Value>(&bytes)
        .map_err(|e| catalog_error(format!("{url}: catalog.json is not JSON: {e}")))?
        .get("format")
        .and_then(serde_json::Value::as_u64);
    match format {
        Some(f) if f > u64::from(FORMAT) => {
            return Err(catalog_error(format!("{url}: catalog form {f} needs a newer aktujo")));
        }
        None => return Err(catalog_error(format!("{url}: catalog.json has no form"))),
        Some(_) => {}
    }
    serde_json::from_slice(&bytes).map_err(|e| catalog_error(format!("{url}: catalog.json: {e}")))
}

/// Reads a template of a source and checks it: its file must have the SHA-256 the catalog names, its kinds must be
/// valid.
pub fn fetch_template(url: &str, entry: &Entry) -> Result<Template, AppError> {
    let url = source_url(url)?;
    let path = entry.path.trim_start_matches('/');
    if path.split('/').any(|part| part == ".." || part.is_empty()) {
        return Err(catalog_error(format!("{url}: {:?} is not a path in the catalog", entry.path)));
    }
    let bytes = get(&format!("{}{path}", files_of(&url)))?;
    let template = template_of_file(&bytes)?;
    if template.sha256.hex() != entry.sha256.to_ascii_lowercase() {
        return Err(catalog_error(format!("{url}: {} is not the file the catalog names", entry.id)));
    }
    Ok(template)
}

/// Reads a file of kinds: a template (`{"author": …, "kinds": […]}`), an export of a space (`{"kinds": […]}`) or a
/// list of kinds; every kind is checked.
pub fn template_of_file(bytes: &[u8]) -> Result<Template, AppError> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum File {
        Kinds { kinds: Vec<Kind>, author: Option<String> },
        List(Vec<Kind>),
    }
    let file: File = serde_json::from_slice(bytes).map_err(|e| catalog_error(format!("not a file of kinds: {e}")))?;
    let (kinds, is_template) = match file {
        File::Kinds { kinds, author } => (kinds, author.is_some()),
        File::List(kinds) => (kinds, false),
    };
    for kind in &kinds {
        kind.check()?;
    }
    Ok(Template { kinds, sha256: Sha256Hash::of(bytes), is_template })
}

/// What taking a template over does to one kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Step {
    /// The space does not have it: added.
    New,
    /// The space has it as the template does.
    Same,
    /// The space has a kind of that identity a person did not change since the template: replaced by the
    /// template's – or one that did not come from this template.
    Replace,
    /// The space has it changed by a person: merged.
    Merge,
    /// It came from this template, which no longer has it: offered to remove.
    Gone,
}

/// One kind of a plan.
#[derive(Debug, Clone, PartialEq)]
pub struct Planned {
    /// The identity.
    pub id: String,
    /// What happens to it.
    pub step: Step,
    /// The space's kind, if it has one.
    pub mine: Option<Kind>,
    /// The template's kind, if it has one.
    pub theirs: Option<Kind>,
    /// The kind as it would be if the step is taken: the template's, or the merge.
    pub proposed: Option<Kind>,
    /// Where a merge kept the space's version of an item the template changed too.
    pub conflicts: Vec<Conflict>,
}

/// What taking a template over would do, kind by kind.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// Where the template comes from.
    pub origin: Origin,
    /// The SHA-256 of its file – remembered, if it is a template.
    pub sha256: Sha256Hash,
    /// Whether its kinds remember it ([`Template::is_template`]).
    pub remember: bool,
    /// Every kind of the template, then those of the space that are gone from it.
    pub kinds: Vec<Planned>,
}

/// What a person chooses for a kind of a plan.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Choice {
    /// What the plan proposes: add, replace, merge – but a kind gone from the template is kept.
    #[default]
    Take,
    /// The template's kind as it is, dropping a person's changes.
    Theirs,
    /// The space's kind as it is; a new one is not added.
    Keep,
    /// A kind gone from the template is removed.
    Remove,
}

/// A template of a source that is new to a space, taken over, or has an update.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Standing {
    /// None of its kinds came from it.
    New,
    /// Its kinds came from it, as it is now.
    Taken,
    /// Its kinds came from an earlier version.
    Update,
}

impl Workspace {
    /// The sources of this device: the official catalog first, then the others in the order they were added.
    pub fn sources(&self) -> Result<Vec<Source>, AppError> {
        let stored: Vec<Source> = match self.replica.store().setting(None, SOURCES)? {
            Some(json) => serde_json::from_str(&json).map_err(|e| catalog_error(format!("the sources: {e}")))?,
            None => vec![],
        };
        let default = stored.iter().find(|s| s.is_default()).cloned();
        let mut sources = vec![default.unwrap_or(Source { url: DEFAULT_SOURCE.into(), on: true })];
        sources.extend(stored.into_iter().filter(|s| !s.is_default()));
        Ok(sources)
    }

    fn keep_sources(&mut self, sources: &[Source]) -> Result<(), AppError> {
        let json = serde_json::to_string(sources).expect("sources are JSON");
        Ok(self.replica.store().set_setting(None, SOURCES, &json)?)
    }

    /// Adds a source, switched on. Returns its address as it is kept.
    pub fn add_source(&mut self, url: &str) -> Result<String, AppError> {
        let url = source_url(url)?;
        let mut sources = self.sources()?;
        if !sources.iter().any(|s| s.url == url) {
            sources.push(Source { url: url.clone(), on: true });
            self.keep_sources(&sources)?;
        }
        Ok(url)
    }

    /// Switches a source on or off – the official catalog too.
    pub fn set_source(&mut self, url: &str, on: bool) -> Result<(), AppError> {
        let url = source_url(url)?;
        let mut sources = self.sources()?;
        let source =
            sources.iter_mut().find(|s| s.url == url).ok_or_else(|| AppError::NotFound(format!("no source {url}")))?;
        source.on = on;
        self.keep_sources(&sources)
    }

    /// Removes a source; the official catalog cannot be removed, only switched off.
    pub fn remove_source(&mut self, url: &str) -> Result<(), AppError> {
        let url = source_url(url)?;
        if url == DEFAULT_SOURCE {
            return Err(AppError::NotAllowed("the official catalog can be switched off, not removed".into()));
        }
        let mut sources = self.sources()?;
        let before = sources.len();
        sources.retain(|s| s.url != url);
        if sources.len() == before {
            return Err(AppError::NotFound(format!("no source {url}")));
        }
        self.keep_sources(&sources)
    }

    /// How a template of a catalog stands in a space: new, taken over as it is, or with an update. A kind that came
    /// from a file counts for the template with that file's SHA-256.
    pub fn standing(&self, space: &SpaceId, source: &str, entry: &Entry) -> Result<Standing, AppError> {
        let kinds = &self.view(space)?.kinds;
        let mut standing = Standing::New;
        for kind in kinds.iter() {
            let Some(template) = kinds.template(&kind.id) else { continue };
            let same_sha = template.sha256.hex() == entry.sha256.to_ascii_lowercase();
            let from_it = template.source.as_deref() == Some(source) && template.id.as_deref() == Some(&entry.id);
            if from_it || (template.source.is_none() && same_sha) {
                if !same_sha {
                    return Ok(Standing::Update);
                }
                standing = Standing::Taken;
            }
        }
        Ok(standing)
    }

    /// What taking `template` over into a space would do, kind by kind. Nothing is written.
    pub fn plan(&self, space: &SpaceId, origin: Origin, template: &Template) -> Result<Plan, AppError> {
        let kinds = &self.view(space)?.kinds;
        let mut planned = Vec::new();
        for theirs in &template.kinds {
            let mine = kinds.get(&theirs.id);
            let base = kinds.template(&theirs.id).map(|t| &t.base);
            let (step, proposed, conflicts) = match (mine, base) {
                (None, _) => (Step::New, Some(theirs.clone()), vec![]),
                (Some(mine), _) if mine == theirs => (Step::Same, Some(theirs.clone()), vec![]),
                (Some(mine), Some(base)) if mine != base.as_ref() => {
                    let merged = merge(base, mine, theirs);
                    (Step::Merge, Some(merged.kind), merged.conflicts)
                }
                (Some(_), _) => (Step::Replace, Some(theirs.clone()), vec![]),
            };
            planned.push(Planned {
                id: theirs.id.clone(),
                step,
                mine: mine.cloned(),
                theirs: Some(theirs.clone()),
                proposed,
                conflicts,
            });
        }
        // Kinds that came from this template and are no longer in it.
        if template.is_template {
            for mine in kinds.iter() {
                let Some(known) = kinds.template(&mine.id) else { continue };
                let from_it = match (&origin.source, &origin.id) {
                    (Some(source), Some(id)) => {
                        known.source.as_deref() == Some(source) && known.id.as_deref() == Some(id.as_str())
                    }
                    _ => false,
                };
                if from_it && !template.kinds.iter().any(|k| k.id == mine.id) {
                    planned.push(Planned {
                        id: mine.id.clone(),
                        step: Step::Gone,
                        mine: Some(mine.clone()),
                        theirs: None,
                        proposed: None,
                        conflicts: vec![],
                    });
                }
            }
        }
        Ok(Plan { origin, sha256: template.sha256, remember: template.is_template, kinds: planned })
    }

    /// Takes a plan over as chosen, kind by kind ([`Choice::Take`] for a kind not named). Each kind written
    /// remembers the template, if the plan is of one – so that the same version is not offered again. Returns how
    /// many kinds changed.
    pub fn apply(
        &mut self,
        space: &SpaceId,
        plan: &Plan,
        choices: &BTreeMap<String, Choice>,
    ) -> Result<usize, AppError> {
        self.may_write(space)?;
        let mut changed = 0;
        for planned in &plan.kinds {
            let choice = choices.get(&planned.id).copied().unwrap_or_default();
            let remembered = |base: &Kind| {
                plan.remember.then(|| TemplateRef {
                    source: plan.origin.source.clone(),
                    id: plan.origin.id.clone(),
                    sha256: plan.sha256,
                    base: Box::new(base.clone()),
                })
            };
            let write = match (planned.step, choice, &planned.theirs) {
                (Step::Gone, Choice::Remove, _) => {
                    self.remove_kind(space, &planned.id)?;
                    changed += 1;
                    None
                }
                // Kept: it remembers this version, so that it is not offered again until the next.
                (Step::Gone, _, _) => planned.mine.clone().map(|mine| {
                    let template = remembered(&mine);
                    (mine, template)
                }),
                (_, Choice::Remove, _) | (Step::New, Choice::Keep, _) | (_, _, None) => None,
                (_, Choice::Keep, Some(theirs)) => planned.mine.clone().map(|mine| (mine, remembered(theirs))),
                (_, Choice::Theirs, Some(theirs)) => Some((theirs.clone(), remembered(theirs))),
                (_, Choice::Take, Some(theirs)) => planned.proposed.clone().map(|kind| (kind, remembered(theirs))),
            };
            let Some((kind, template)) = write else { continue };
            let kinds = &self.view(space)?.kinds;
            let same_kind = kinds.get(&kind.id) == Some(&kind);
            let same_template = template.is_none() || kinds.template(&kind.id) == template.as_ref();
            if same_kind && same_template {
                continue;
            }
            if !same_kind {
                changed += 1;
            }
            self.write_kind(space, kind, template)?;
        }
        Ok(changed)
    }

    /// Takes over a file of kinds, as chosen nothing: what the plan proposes. A template remembers its SHA-256 and
    /// base. Returns how many kinds changed.
    pub fn import_file(&mut self, space: &SpaceId, bytes: &[u8]) -> Result<usize, AppError> {
        let template = template_of_file(bytes)?;
        let plan = self.plan(space, Origin::default(), &template)?;
        self.apply(space, &plan, &BTreeMap::new())
    }
}

#[cfg(test)]
mod tests {
    use aktujo_doc::{FieldDef, FieldType};

    use super::*;

    fn letter() -> Kind {
        Kind::new(
            "letter",
            "Letter",
            vec![FieldDef::new("sender", "Sender", FieldType::Text), FieldDef::new("date", "Date", FieldType::Date)],
        )
    }

    fn file(kinds: &[Kind]) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({"author": "aktujo", "name": "Letters", "kinds": kinds})).unwrap()
    }

    #[test]
    fn sources_keep_the_official_catalog() {
        let mut workspace = Workspace::in_memory("laptop").unwrap();
        assert_eq!(workspace.sources().unwrap(), vec![Source { url: DEFAULT_SOURCE.into(), on: true }]);
        assert_eq!(workspace.add_source(" https://example.org/kinds/ ").unwrap(), "https://example.org/kinds");
        assert!(workspace.add_source("http://example.org").is_err(), "only HTTPS, or HTTP on this device");
        assert!(workspace.add_source("http://localhost:8080").is_ok());
        workspace.set_source(DEFAULT_SOURCE, false).unwrap();
        let sources = workspace.sources().unwrap();
        assert_eq!(sources[0], Source { url: DEFAULT_SOURCE.into(), on: false });
        assert_eq!(sources.len(), 3);
        assert!(matches!(workspace.remove_source(DEFAULT_SOURCE), Err(AppError::NotAllowed(_))));
        workspace.remove_source("https://example.org/kinds").unwrap();
        assert_eq!(workspace.sources().unwrap().len(), 2);
    }

    #[test]
    fn sources_on_github_are_read_from_their_raw_files() {
        assert_eq!(files_of(DEFAULT_SOURCE), "https://raw.githubusercontent.com/aktujo/catalog/HEAD/");
        assert_eq!(
            files_of("https://github.com/someone/kinds/tree/next"),
            "https://raw.githubusercontent.com/someone/kinds/next/"
        );
        assert_eq!(files_of("https://example.org/kinds"), "https://example.org/kinds/");
    }

    #[test]
    fn a_file_is_taken_over_and_a_newer_version_merged_with_a_persons_change() {
        let mut workspace = Workspace::in_memory("laptop").unwrap();
        let space = workspace.found_space("Home").unwrap();
        let first = file(&[letter()]);
        assert_eq!(workspace.import_file(&space, &first).unwrap(), 1);
        let remembered = workspace.view(&space).unwrap().kinds.template("letter").cloned().unwrap();
        assert_eq!((remembered.source, remembered.sha256), (None, Sha256Hash::of(&first)));

        // A person renames the kind; the template gets a new field.
        let mut mine = letter();
        mine.name = "Mail".into();
        workspace.define_kind(&space, mine).unwrap();
        let mut newer = letter();
        newer.fields.push(FieldDef::new("subject", "Subject", FieldType::Text));
        let template = template_of_file(&file(&[newer.clone()])).unwrap();
        let plan = workspace.plan(&space, Origin::default(), &template).unwrap();
        assert_eq!(plan.kinds[0].step, Step::Merge);
        assert!(plan.kinds[0].conflicts.is_empty());
        assert_eq!(workspace.apply(&space, &plan, &BTreeMap::new()).unwrap(), 1);
        let kind = workspace.kind(&space, "letter").unwrap().clone();
        assert_eq!((kind.name.as_str(), kind.fields.len()), ("Mail", 3));
        // Taken: the same version is not offered again.
        let again = workspace.plan(&space, Origin::default(), &template).unwrap();
        assert_eq!(again.kinds[0].step, Step::Merge, "still changed by the person");
        assert_eq!(workspace.apply(&space, &again, &BTreeMap::new()).unwrap(), 0);

        // The template's own version, dropping the person's change.
        let choices = BTreeMap::from([("letter".to_owned(), Choice::Theirs)]);
        workspace.apply(&space, &plan, &choices).unwrap();
        assert_eq!(workspace.kind(&space, "letter").unwrap(), &newer);
    }

    #[test]
    fn a_template_of_a_catalog_is_new_taken_updated_and_its_gone_kinds_offered() {
        let mut workspace = Workspace::in_memory("laptop").unwrap();
        let space = workspace.found_space("Home").unwrap();
        let book = Kind::new("book", "Book", vec![FieldDef::new("isbn", "ISBN", FieldType::Text)]);
        let first = template_of_file(&file(&[letter(), book.clone()])).unwrap();
        let origin = Origin { source: Some(DEFAULT_SOURCE.into()), id: Some("aktujo.letters".into()) };
        let entry = |template: &Template| Entry {
            id: "aktujo.letters".into(),
            author: "aktujo".into(),
            path: "templates/aktujo/letters/template.json".into(),
            sha256: template.sha256.hex(),
            license: Some("MIT".into()),
            name: "Letters".into(),
            names: BTreeMap::new(),
            description: String::new(),
            descriptions: BTreeMap::new(),
            tags: vec![],
            kinds: vec![],
        };
        assert_eq!(workspace.standing(&space, DEFAULT_SOURCE, &entry(&first)).unwrap(), Standing::New);
        let plan = workspace.plan(&space, origin.clone(), &first).unwrap();
        assert_eq!(plan.kinds.iter().map(|k| k.step).collect::<Vec<_>>(), [Step::New, Step::New]);
        // The person does not want books.
        let choices = BTreeMap::from([("book".to_owned(), Choice::Keep)]);
        assert_eq!(workspace.apply(&space, &plan, &choices).unwrap(), 1);
        assert_eq!(workspace.kinds(&space).unwrap(), vec![letter()]);
        assert_eq!(workspace.standing(&space, DEFAULT_SOURCE, &entry(&first)).unwrap(), Standing::Taken);

        // A newer version without the letter.
        let second = template_of_file(&file(&[book])).unwrap();
        assert_eq!(workspace.standing(&space, DEFAULT_SOURCE, &entry(&second)).unwrap(), Standing::Update);
        let plan = workspace.plan(&space, origin, &second).unwrap();
        let steps: Vec<(&str, Step)> = plan.kinds.iter().map(|k| (k.id.as_str(), k.step)).collect();
        assert_eq!(steps, [("book", Step::New), ("letter", Step::Gone)]);
        // Gone is kept unless chosen: it remembers the new version and is not offered again.
        workspace.apply(&space, &plan, &BTreeMap::from([("book".to_owned(), Choice::Keep)])).unwrap();
        assert!(workspace.kind(&space, "letter").is_ok());
        assert_eq!(workspace.standing(&space, DEFAULT_SOURCE, &entry(&second)).unwrap(), Standing::Taken);
        let choices = BTreeMap::from([("letter".to_owned(), Choice::Remove), ("book".to_owned(), Choice::Keep)]);
        workspace.apply(&space, &plan, &choices).unwrap();
        assert!(workspace.kinds(&space).unwrap().is_empty());
    }

    #[test]
    fn an_export_or_a_list_remembers_nothing() {
        let mut workspace = Workspace::in_memory("laptop").unwrap();
        let space = workspace.found_space("Home").unwrap();
        let export = serde_json::to_vec(&serde_json::json!({"name": "Home", "kinds": [letter()]})).unwrap();
        assert_eq!(workspace.import_file(&space, &export).unwrap(), 1);
        assert!(workspace.view(&space).unwrap().kinds.template("letter").is_none());
        assert!(template_of_file(b"{\"kinds\": [{\"id\": \"Bad\", \"name\": \"x\", \"fields\": []}]}").is_err());
    }
}
