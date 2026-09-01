//! Hardcoded unified model alias registry.
//!
//! Outbound clients should send stable lowercase kebab-case aliases. The
//! original OpenCode Go protocol table plus sealed Provider adapter maps are
//! the built-in Alias authority. Refreshed Provider catalogs may join only
//! those code-owned names and never create arbitrary new ones. Case-folded
//! kebab spellings such as `GLM-5.2` are accepted. Names containing `/`, `_`,
//! or whitespace are treated as raw IDs and never folded onto a kebab alias
//! (`glm/5.2` is not `glm-5.2`). A raw upstream model ID is accepted only
//! when it uniquely selects one provider mapping; ambiguity returns
//! [`ResolveError::Ambiguous`] with code [`AMBIGUOUS_MODEL_ID`].
//!
//! Command Code GOAT rows join a code-owned Alias by leaf name where possible.
//! Known plan suffixes are stripped only when the shorter Alias is authorized;
//! selected verbose implementation IDs use a sealed exact map. The unique
//! slash raw ID still pins to GOAT. Statically authorized Zen `*-free` IDs likewise
//! stay exact raw pins while joining only their stripped Go-table Alias. Eligible Custom capabilities
//! overlay published aliases and resolve otherwise unknown IDs without
//! stealing Go/Zen mappings.
//! Later host adapters consume [`ProviderMapping`]: parse the client protocol
//! once, then materialize model / protocol / endpoint / auth per candidate.
//! Adapters must not probe a billable inference path to discover protocol
//! support. The OpenCode protocol table stays Go-specific.
//!
//! Items are rust-public only as the cross-crate bridge; the host crate's
//! `alias` compatibility facade keeps the historical public paths.

use ocg_domain::ids::{
    COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_ALIAS, COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM,
    COMMAND_CODE_PROVIDER_ID, CPA_PROVIDER_ID, CUSTOM_PROVIDER_ID, KIMI_PROVIDER_ID,
    MINIMAX_PROVIDER_ID, OPENCODE_PROVIDER_ID, OPENCODE_ZEN_FREE_PROVIDER_ID,
    custom_model_id_matches, is_free_model, looks_raw_shaped,
};
use ocg_domain::protocol::supported_model_ids;
use ocg_domain::provider::is_custom_api;
use ocg_domain::zen::{ZenFreeModelCatalog, stripped_free_alias};
use std::collections::BTreeMap;
use std::sync::OnceLock;

const MINIMAX_CN_ALIASES: &[(&str, &str)] = &[
    ("MiniMax-M3", "minimax-m3"),
    ("MiniMax-M2.7", "minimax-m2.7"),
    ("MiniMax-M2.7-highspeed", "minimax-m2.7-highspeed"),
    ("MiniMax-M2.5", "minimax-m2.5"),
    ("MiniMax-M2.5-highspeed", "minimax-m2.5-highspeed"),
    ("MiniMax-M2.1", "minimax-m2.1"),
    ("MiniMax-M2.1-highspeed", "minimax-m2.1-highspeed"),
    ("MiniMax-M2", "minimax-m2"),
];

const KIMI_CN_ALIASES: &[(&str, &str)] = &[
    ("kimi-for-coding", "kimi-k2.7-code"),
    ("kimi-for-coding-highspeed", "kimi-k2.7-code-highspeed"),
    ("k3", "kimi-k3"),
    ("k3-256k", "kimi-k3-256k"),
];

/// GOAT-only names whose upstream leaf contains an implementation qualifier
/// that is not part of the stable client model family. The exact upstream ID
/// keeps working as a pinned raw request.
const COMMAND_CODE_GOAT_ALIASES: &[(&str, &str)] =
    &[("nvidia/nemotron-3-ultra-550b-a55b", "nemotron-3-ultra")];

/// Machine-readable error code for a raw ID that matches more than one mapping.
pub const AMBIGUOUS_MODEL_ID: &str = "ambiguous_model_id";

/// One provider's upstream identity for a client-facing name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderMapping {
    pub provider_id: String,
    pub upstream_model: String,
    /// Production-routeable mappings only. Reserved providers stay false.
    pub routeable: bool,
}

/// One extra (non-sealed) provider's public-to-upstream mappings.
///
/// Used by dynamic Providers. Built-in and Custom catalogs stay on the named
/// fields of [`RuntimeCatalogs`]; this collection avoids adding a new optional
/// field per Provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtraProviderCatalog {
    pub provider_id: String,
    pub mappings: Vec<(String, String)>,
}

/// Borrowed runtime catalog inputs used to overlay the sealed Alias registry.
///
/// Callers pass one value instead of extending resolver signatures whenever a
/// new static adapter contributes a catalog. The registry remains code-owned;
/// extra catalogs are a generic owned-string collection, not a plugin slot.
#[derive(Debug, Clone, Copy, Default)]
pub struct RuntimeCatalogs<'a> {
    pub go: &'a [String],
    pub zen_free: &'a [String],
    pub custom: &'a [String],
    pub command_code: &'a [String],
    pub minimax: &'a [String],
    pub kimi: &'a [String],
    pub cpa: &'a [String],
    pub extra: &'a [ExtraProviderCatalog],
}

impl ProviderMapping {
    pub fn is_opencode_go(&self) -> bool {
        self.provider_id == OPENCODE_PROVIDER_ID
    }

    pub fn is_zen_free(&self) -> bool {
        self.provider_id == OPENCODE_ZEN_FREE_PROVIDER_ID
    }

    pub fn is_command_code_goat(&self) -> bool {
        ocg_domain::provider::is_command_code_goat(&self.provider_id)
    }

    pub fn is_custom_api(&self) -> bool {
        is_custom_api(&self.provider_id)
    }

    pub fn is_minimax_cn(&self) -> bool {
        self.provider_id == MINIMAX_PROVIDER_ID
    }

    pub fn is_kimi_cn(&self) -> bool {
        self.provider_id == KIMI_PROVIDER_ID
    }

    pub fn is_cpa(&self) -> bool {
        self.provider_id == CPA_PROVIDER_ID
    }
}

/// A preferred client-facing alias and its provider mappings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AliasEntry {
    pub alias: String,
    pub mappings: Vec<ProviderMapping>,
}

/// Result of looking up a client-supplied model name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedModel {
    /// Preferred alias. May follow account order, sticky, and fallback across
    /// routeable mappings (including Zen prefer overlay).
    Alias {
        requested: String,
        alias: String,
        mappings: Vec<ProviderMapping>,
    },
    /// Raw upstream ID that uniquely selected one routeable mapping. Pinned to
    /// that provider; no cross-provider fallback or prefer overlay.
    PinnedRaw {
        requested: String,
        mapping: ProviderMapping,
    },
}

impl ResolvedModel {
    pub fn requested(&self) -> &str {
        match self {
            Self::Alias { requested, .. } | Self::PinnedRaw { requested, .. } => requested,
        }
    }

    pub fn is_pinned(&self) -> bool {
        matches!(self, Self::PinnedRaw { .. })
    }

    pub fn routeable_mappings(&self) -> Vec<&ProviderMapping> {
        match self {
            Self::Alias { mappings, .. } => mappings
                .iter()
                .filter(|mapping| mapping.routeable)
                .collect(),
            Self::PinnedRaw { mapping, .. } if mapping.routeable => vec![mapping],
            Self::PinnedRaw { .. } => Vec::new(),
        }
    }

    /// Alias requests may follow account order, sticky, and fallback.
    /// Unique raw IDs stay pinned to one provider mapping.
    pub fn allows_cross_account_fallback(&self) -> bool {
        matches!(self, Self::Alias { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    Unknown {
        requested: String,
    },
    Ambiguous {
        requested: String,
        mappings: Vec<ProviderMapping>,
    },
}

impl ResolveError {
    pub fn code(&self) -> Option<&'static str> {
        match self {
            Self::Ambiguous { .. } => Some(AMBIGUOUS_MODEL_ID),
            Self::Unknown { .. } => None,
        }
    }

    pub fn message(&self) -> String {
        match self {
            Self::Unknown { requested } => format!("unknown model `{requested}`"),
            Self::Ambiguous {
                requested,
                mappings,
            } => {
                let providers = mappings
                    .iter()
                    .map(|mapping| format!("{}:{}", mapping.provider_id, mapping.upstream_model))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "{AMBIGUOUS_MODEL_ID}: requested model `{requested}` matches multiple provider mappings ({providers}); send a preferred alias instead of this raw id"
                )
            }
        }
    }
}

static REGISTRY: OnceLock<Registry> = OnceLock::new();

fn registry() -> &'static Registry {
    REGISTRY.get_or_init(build_builtin_registry)
}

fn build_builtin_registry() -> Registry {
    build_registry(&ZenFreeModelCatalog::default().models)
}

fn build_registry(zen_free_models: &[String]) -> Registry {
    let mut specs = Vec::new();
    let static_zen_aliases = supported_model_ids()
        .filter(|id| is_free_model(id))
        .filter_map(stripped_free_alias)
        .map(|alias| alias.to_ascii_lowercase())
        .collect::<std::collections::HashSet<_>>();
    for id in supported_model_ids() {
        if id == "big-pickle" || is_free_model(id) {
            continue;
        } else {
            specs.push(AliasEntry {
                alias: id.to_string(),
                mappings: go_alias_mappings(id),
            });
        }
    }
    let mut registry = registry_from_entries(specs);
    for model in zen_free_models {
        if !is_free_model(model) {
            continue;
        }
        if let Some(alias) = stripped_free_alias(model) {
            let mapping = zen_mapping(model);
            insert_raw_mapping(&mut registry, mapping.clone());
            let key = alias.to_ascii_lowercase();
            if registry.aliases.contains_key(&key) || static_zen_aliases.contains(&key) {
                insert_mapping(&mut registry, alias, mapping);
            }
        }
    }
    registry
}

fn build_runtime_registry(catalogs: RuntimeCatalogs<'_>) -> Registry {
    let mut registry = build_registry(catalogs.zen_free);
    insert_sealed_catalog(
        &mut registry,
        catalogs.minimax,
        minimax_mapping,
        minimax_catalog_alias,
    );
    insert_sealed_catalog(
        &mut registry,
        catalogs.kimi,
        kimi_mapping,
        kimi_catalog_alias,
    );
    insert_goat_catalog(&mut registry, catalogs.command_code);
    insert_cpa_catalog(&mut registry, catalogs.cpa);
    insert_extra_catalogs(&mut registry, catalogs.extra);
    registry
}

fn insert_extra_catalogs(registry: &mut Registry, extras: &[ExtraProviderCatalog]) {
    for extra in extras {
        for (public_model, upstream_model) in &extra.mappings {
            let provider_mapping = mapping(&extra.provider_id, upstream_model, true);
            insert_raw_mapping(registry, provider_mapping.clone());
            if !looks_raw_shaped(public_model) {
                insert_mapping(registry, public_model, provider_mapping);
            } else {
                insert_raw_mapping(registry, mapping(&extra.provider_id, public_model, true));
            }
        }
    }
}

fn insert_goat_catalog(registry: &mut Registry, model_ids: &[String]) {
    for model_id in model_ids {
        upsert_mapping(registry, None, goat_mapping(model_id, true));
    }
    for model_id in model_ids {
        let alias = command_alias_for_catalog(model_id, registry);
        let unique = model_ids
            .iter()
            .filter(|candidate| {
                command_alias_for_catalog(candidate, registry).eq_ignore_ascii_case(&alias)
            })
            .count()
            == 1;
        if unique && is_code_owned_alias(registry, &alias) {
            upsert_mapping(registry, Some(&alias), goat_mapping(model_id, true));
        }
    }
}

fn insert_sealed_catalog(
    registry: &mut Registry,
    model_ids: &[String],
    mapping: fn(&str) -> ProviderMapping,
    alias_for_model: fn(&str) -> Option<&'static str>,
) {
    for model_id in model_ids {
        let provider_mapping = mapping(model_id);
        insert_raw_mapping(registry, provider_mapping.clone());
        if let Some(alias) = alias_for_model(model_id) {
            insert_mapping(registry, alias, provider_mapping);
        }
    }
}

/// CPA catalog rows may join only an already code-owned Alias. Every row also
/// keeps its exact upstream raw pin; CPA never authors arbitrary aliases.
fn insert_cpa_catalog(registry: &mut Registry, model_ids: &[String]) {
    for model_id in model_ids {
        let mapping = cpa_mapping(model_id);
        let alias = code_owned_alias(registry, model_id);
        insert_raw_mapping(registry, mapping.clone());
        if let Some(alias) = alias {
            insert_mapping(registry, &alias, mapping);
        }
    }
}

fn mapping(provider_id: &str, upstream_model: &str, routeable: bool) -> ProviderMapping {
    ProviderMapping {
        provider_id: provider_id.to_string(),
        upstream_model: upstream_model.to_string(),
        routeable,
    }
}

fn go_mapping(upstream_model: &str) -> ProviderMapping {
    mapping(OPENCODE_PROVIDER_ID, upstream_model, true)
}

fn goat_deepseek_v4_flash_mapping() -> ProviderMapping {
    goat_mapping(COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM, false)
}

fn goat_mapping(upstream_model: &str, routeable: bool) -> ProviderMapping {
    mapping(COMMAND_CODE_PROVIDER_ID, upstream_model, routeable)
}

fn goat_catalog_hit(goat_model_ids: &[String], requested: &str) -> Option<String> {
    goat_model_ids
        .iter()
        .find(|id| id.trim() == requested.trim())
        .cloned()
}

fn goat_catalog_hit_for_resolved(
    goat_model_ids: &[String],
    resolved: &ResolvedModel,
    registry: &Registry,
) -> Option<String> {
    goat_catalog_hit(goat_model_ids, resolved.requested()).or_else(|| match resolved {
        ResolvedModel::Alias {
            alias, mappings, ..
        } => mappings
            .iter()
            .filter(|mapping| mapping.is_command_code_goat())
            .find_map(|mapping| goat_catalog_hit(goat_model_ids, &mapping.upstream_model))
            .or_else(|| command_catalog_hit_for_alias(goat_model_ids, alias, registry)),
        ResolvedModel::PinnedRaw { mapping, .. } if mapping.is_command_code_goat() => {
            goat_catalog_hit(goat_model_ids, &mapping.upstream_model)
        }
        _ => None,
    })
}

const COMMAND_ALIAS_SUFFIX_EXCEPTIONS: &[&str] = &["-paid", "-free"];
const COMMAND_ALIAS_EXACT_EXCEPTIONS: &[(&str, &str)] = &[("ox-alpha", "ox-alpha-free")];

fn command_catalog_hit_for_alias(
    goat_model_ids: &[String],
    alias: &str,
    registry: &Registry,
) -> Option<String> {
    let matches = goat_model_ids
        .iter()
        .filter(|id| command_alias_for_catalog(id, registry).eq_ignore_ascii_case(alias))
        .collect::<Vec<_>>();
    (matches.len() == 1).then(|| matches[0].clone())
}

fn go_alias_mappings(upstream_model: &'static str) -> Vec<ProviderMapping> {
    let mut mappings = vec![go_mapping(upstream_model)];
    if upstream_model == COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_ALIAS {
        mappings.push(goat_deepseek_v4_flash_mapping());
    }
    mappings
}

fn zen_mapping(upstream_model: &str) -> ProviderMapping {
    mapping(OPENCODE_ZEN_FREE_PROVIDER_ID, upstream_model, true)
}

fn custom_mapping(upstream_model: &str) -> ProviderMapping {
    mapping(CUSTOM_PROVIDER_ID, upstream_model, true)
}

fn minimax_mapping(upstream_model: &str) -> ProviderMapping {
    mapping(MINIMAX_PROVIDER_ID, upstream_model, true)
}

fn kimi_mapping(upstream_model: &str) -> ProviderMapping {
    mapping(KIMI_PROVIDER_ID, upstream_model, true)
}

fn cpa_mapping(upstream_model: &str) -> ProviderMapping {
    mapping(CPA_PROVIDER_ID, upstream_model, true)
}

/// Sentinel upstream id for Custom-only resolutions. Per-candidate materialization
/// uses the account's declared capability ID instead of this value.
pub const CUSTOM_DYNAMIC_UPSTREAM: &str = "";

struct Registry {
    aliases: BTreeMap<String, AliasEntry>,
    /// Exact upstream model ID → every mapping that uses it.
    raw_exact: BTreeMap<String, Vec<ProviderMapping>>,
}

fn registry_from_entries(entries: Vec<AliasEntry>) -> Registry {
    let mut aliases = BTreeMap::new();
    let mut raw_exact: BTreeMap<String, Vec<ProviderMapping>> = BTreeMap::new();
    for entry in entries {
        debug_assert!(
            !looks_raw_shaped(&entry.alias),
            "published aliases must be kebab-case without slash, space, or underscore"
        );
        for mapping in &entry.mappings {
            raw_exact
                .entry(mapping.upstream_model.to_string())
                .or_default()
                .push(mapping.clone());
        }
        aliases.insert(entry.alias.to_lowercase(), entry);
    }
    Registry { aliases, raw_exact }
}

fn insert_mapping(registry: &mut Registry, alias: &str, mapping: ProviderMapping) {
    let key = alias.to_ascii_lowercase();
    let entry = registry.aliases.entry(key).or_insert_with(|| AliasEntry {
        alias: alias.to_string(),
        mappings: Vec::new(),
    });
    if !entry.mappings.contains(&mapping) {
        entry.mappings.push(mapping.clone());
    }
    insert_raw_mapping(registry, mapping);
}

fn upsert_mapping(registry: &mut Registry, alias: Option<&str>, mapping: ProviderMapping) {
    let same_identity = |existing: &ProviderMapping| {
        existing.provider_id == mapping.provider_id
            && existing.upstream_model == mapping.upstream_model
    };
    if let Some(alias) = alias {
        let key = alias.to_ascii_lowercase();
        let entry = registry.aliases.entry(key).or_insert_with(|| AliasEntry {
            alias: alias.to_string(),
            mappings: Vec::new(),
        });
        if let Some(existing) = entry.mappings.iter_mut().find(|item| same_identity(item)) {
            *existing = mapping.clone();
        } else {
            entry.mappings.push(mapping.clone());
        }
    }
    let mappings = registry
        .raw_exact
        .entry(mapping.upstream_model.clone())
        .or_default();
    if let Some(existing) = mappings.iter_mut().find(|item| same_identity(item)) {
        *existing = mapping;
    } else {
        mappings.push(mapping);
    }
}

fn insert_raw_mapping(registry: &mut Registry, mapping: ProviderMapping) {
    let mappings = registry
        .raw_exact
        .entry(mapping.upstream_model.clone())
        .or_default();
    if !mappings.contains(&mapping) {
        mappings.push(mapping);
    }
}

fn pin_or_ambiguous(
    requested: String,
    mappings: &[ProviderMapping],
) -> Result<ResolvedModel, ResolveError> {
    match mappings {
        [mapping] => Ok(ResolvedModel::PinnedRaw {
            requested,
            mapping: mapping.clone(),
        }),
        [] => Err(ResolveError::Unknown { requested }),
        _ => Err(ResolveError::Ambiguous {
            requested,
            mappings: mappings.to_vec(),
        }),
    }
}

/// Resolve a client-supplied model name against the builtin registry.
pub fn resolve(requested: &str) -> Result<ResolvedModel, ResolveError> {
    resolve_in(registry(), requested)
}

/// Resolve against the builtin registry, then overlay eligible Custom
/// capability IDs. Published aliases keep their Go/Zen mappings and gain
/// compatible Custom candidates. Distinct provider raw-ID conflicts stay
/// [`ResolveError::Ambiguous`]. Unknown names resolve from Custom only.
pub fn resolve_with_custom(
    requested: &str,
    custom_model_ids: &[String],
) -> Result<ResolvedModel, ResolveError> {
    let custom_alias = custom_model_ids
        .iter()
        .find(|id| custom_model_id_matches(id, requested))
        .cloned();
    let custom_hit = custom_alias.is_some();
    match resolve(requested) {
        Ok(ResolvedModel::Alias {
            requested,
            alias,
            mut mappings,
        }) => {
            if custom_hit && !mappings.iter().any(|mapping| mapping.is_custom_api()) {
                mappings.push(custom_mapping(&alias));
            }
            Ok(ResolvedModel::Alias {
                requested,
                alias,
                mappings,
            })
        }
        Ok(ResolvedModel::PinnedRaw { requested, mapping }) => {
            if custom_hit && !mapping.is_custom_api() {
                return Err(ResolveError::Ambiguous {
                    requested,
                    mappings: vec![mapping, custom_mapping(CUSTOM_DYNAMIC_UPSTREAM)],
                });
            }
            Ok(ResolvedModel::PinnedRaw { requested, mapping })
        }
        Err(ResolveError::Unknown { requested }) if custom_hit => Ok(ResolvedModel::Alias {
            requested,
            alias: custom_alias.expect("custom alias exists when custom_hit is true"),
            mappings: vec![custom_mapping(CUSTOM_DYNAMIC_UPSTREAM)],
        }),
        other => other,
    }
}

/// Resolve against the current persisted Zen Free catalog and eligible Custom
/// capabilities. Zen mappings are rebuilt from the small bounded snapshot so a
/// successful manual refresh takes effect without restarting the Gateway.
pub fn resolve_with_provider_models(
    requested: &str,
    zen_free_models: &[String],
    custom_model_ids: &[String],
) -> Result<ResolvedModel, ResolveError> {
    resolve_with_catalogs(requested, zen_free_models, custom_model_ids, &[])
}

/// Resolve against Zen, Custom, and eligible Command Code GOAT catalog IDs.
/// GOAT overlays never create or steal an Alias, but a verified GOAT mapping
/// may join a name authorized by the original Go table. Other exact catalog IDs
/// pin to GOAT without entering the Alias namespace.
/// Overlapping raw IDs stay [`ResolveError::Ambiguous`].
pub fn resolve_with_catalogs(
    requested: &str,
    zen_free_models: &[String],
    custom_model_ids: &[String],
    goat_model_ids: &[String],
) -> Result<ResolvedModel, ResolveError> {
    resolve_with_all_catalogs(
        requested,
        &[],
        zen_free_models,
        custom_model_ids,
        goat_model_ids,
    )
}

/// Resolve against the persisted OpenCode Go and Zen catalogs plus eligible
/// Custom and GOAT account catalogs. Refreshed catalogs may add exact raw pins
/// or activate only code-owned aliases.
pub fn resolve_with_all_catalogs(
    requested: &str,
    go_model_ids: &[String],
    zen_free_models: &[String],
    custom_model_ids: &[String],
    goat_model_ids: &[String],
) -> Result<ResolvedModel, ResolveError> {
    resolve_with_extended_catalogs(
        requested,
        go_model_ids,
        zen_free_models,
        custom_model_ids,
        goat_model_ids,
        &[],
        &[],
    )
}

/// Extended sealed-provider overlay used by the host. Existing public helpers
/// keep their stable signatures while MiniMax/Kimi catalogs participate in
/// raw-ID ambiguity and may join aliases authorized by the Go table or their
/// own sealed adapter maps.
pub fn resolve_with_extended_catalogs(
    requested: &str,
    go_model_ids: &[String],
    zen_free_models: &[String],
    custom_model_ids: &[String],
    goat_model_ids: &[String],
    minimax_model_ids: &[String],
    kimi_model_ids: &[String],
) -> Result<ResolvedModel, ResolveError> {
    resolve_with_runtime_catalogs(
        requested,
        RuntimeCatalogs {
            go: go_model_ids,
            zen_free: zen_free_models,
            custom: custom_model_ids,
            command_code: goat_model_ids,
            minimax: minimax_model_ids,
            kimi: kimi_model_ids,
            cpa: &[],
            extra: &[],
        },
    )
}

/// Resolve one client model against all runtime catalog inputs.
///
/// Catalog rows may activate code-owned aliases or exact raw pins, but they do
/// not become a dynamic Alias registry and cannot bypass ambiguity checks.
pub fn resolve_with_runtime_catalogs(
    requested: &str,
    catalogs: RuntimeCatalogs<'_>,
) -> Result<ResolvedModel, ResolveError> {
    let registry = build_runtime_registry(catalogs);
    let go_resolved = match resolve_in(&registry, requested) {
        Ok(resolved) => overlay_go_catalog(resolved, catalogs.go),
        Err(ResolveError::Unknown { requested }) => overlay_unknown_go(requested, catalogs.go),
        other => other,
    };
    let custom_resolved = match go_resolved {
        Ok(resolved) => overlay_custom_catalog(resolved, catalogs.custom),
        Err(ResolveError::Unknown { requested }) => match catalogs
            .custom
            .iter()
            .find(|id| custom_model_id_matches(id, &requested))
        {
            Some(alias) => Ok(ResolvedModel::Alias {
                requested,
                alias: alias.clone(),
                mappings: vec![custom_mapping(CUSTOM_DYNAMIC_UPSTREAM)],
            }),
            None => Err(ResolveError::Unknown { requested }),
        },
        other => other,
    };
    let goat_resolved = match custom_resolved {
        Ok(resolved) => overlay_goat_catalog(resolved, catalogs.command_code, &registry),
        Err(ResolveError::Unknown { requested }) => {
            overlay_unknown_goat(requested, catalogs.command_code, &registry)
        }
        other => other,
    };
    match goat_resolved {
        Ok(resolved) => overlay_extra_catalogs(resolved, catalogs.extra),
        Err(ResolveError::Unknown { requested }) => {
            overlay_unknown_extra(requested, catalogs.extra)
        }
        other => other,
    }
}

fn extra_mapping(extra: &ExtraProviderCatalog, upstream_model: &str) -> ProviderMapping {
    mapping(&extra.provider_id, upstream_model, true)
}

fn extra_public_hit<'a>(
    extra: &'a ExtraProviderCatalog,
    requested: &str,
) -> Option<&'a (String, String)> {
    extra
        .mappings
        .iter()
        .find(|(public_model, upstream_model)| {
            custom_model_id_matches(public_model, requested)
                || upstream_model.trim() == requested.trim()
        })
}

fn overlay_extra_catalogs(
    resolved: ResolvedModel,
    extras: &[ExtraProviderCatalog],
) -> Result<ResolvedModel, ResolveError> {
    let mut current = resolved;
    for extra in extras {
        current = overlay_one_extra(current, extra)?;
    }
    Ok(current)
}

fn overlay_one_extra(
    resolved: ResolvedModel,
    extra: &ExtraProviderCatalog,
) -> Result<ResolvedModel, ResolveError> {
    let Some((public_model, upstream_model)) = extra_public_hit(extra, resolved.requested()) else {
        return Ok(resolved);
    };
    let replacement = extra_mapping(extra, upstream_model);
    match resolved {
        ResolvedModel::Alias {
            requested,
            alias,
            mut mappings,
        } => {
            if let Some(existing) = mappings
                .iter_mut()
                .find(|mapping| mapping.provider_id.eq_ignore_ascii_case(&extra.provider_id))
            {
                *existing = replacement;
            } else {
                mappings.push(replacement);
            }
            let _ = public_model;
            Ok(ResolvedModel::Alias {
                requested,
                alias,
                mappings,
            })
        }
        ResolvedModel::PinnedRaw { requested, mapping }
            if !mapping.provider_id.eq_ignore_ascii_case(&extra.provider_id) =>
        {
            Err(ResolveError::Ambiguous {
                requested,
                mappings: vec![mapping, replacement],
            })
        }
        other => Ok(other),
    }
}

fn overlay_unknown_extra(
    requested: String,
    extras: &[ExtraProviderCatalog],
) -> Result<ResolvedModel, ResolveError> {
    let mut public_hits = Vec::new();
    let mut raw_hits = Vec::new();
    for extra in extras {
        for (public_model, upstream_model) in &extra.mappings {
            if custom_model_id_matches(public_model, &requested) {
                public_hits.push((public_model.clone(), extra_mapping(extra, upstream_model)));
            }
            if upstream_model.trim() == requested.trim() {
                raw_hits.push(extra_mapping(extra, upstream_model));
            }
        }
    }
    let exact_raw = raw_hits
        .iter()
        .any(|mapping| mapping.upstream_model.trim() == requested.trim());
    if exact_raw && (looks_raw_shaped(&requested) || public_hits.is_empty()) {
        return pin_or_ambiguous(requested, &raw_hits);
    }
    if !public_hits.is_empty() {
        let alias = public_hits[0].0.clone();
        return Ok(ResolvedModel::Alias {
            requested,
            alias,
            mappings: public_hits
                .into_iter()
                .map(|(_, mapping)| mapping)
                .collect(),
        });
    }
    Err(ResolveError::Unknown { requested })
}

fn overlay_go_catalog(
    resolved: ResolvedModel,
    go_model_ids: &[String],
) -> Result<ResolvedModel, ResolveError> {
    let Some(canonical) = provider_catalog_hit_for_resolved(
        go_model_ids,
        &resolved,
        |mapping| mapping.is_opencode_go(),
        go_catalog_alias,
    ) else {
        return Ok(resolved);
    };
    overlay_known_provider(resolved, canonical, go_mapping)
}

fn overlay_unknown_go(
    requested: String,
    go_model_ids: &[String],
) -> Result<ResolvedModel, ResolveError> {
    overlay_unknown_provider(requested, go_model_ids, go_mapping)
}

fn overlay_custom_catalog(
    resolved: ResolvedModel,
    custom_model_ids: &[String],
) -> Result<ResolvedModel, ResolveError> {
    let custom_hit = custom_model_ids
        .iter()
        .any(|id| custom_model_id_matches(id, resolved.requested()));
    if !custom_hit {
        return Ok(resolved);
    }
    match resolved {
        ResolvedModel::Alias {
            requested,
            alias,
            mut mappings,
        } => {
            if !mappings.iter().any(|mapping| mapping.is_custom_api()) {
                mappings.push(custom_mapping(&alias));
            }
            Ok(ResolvedModel::Alias {
                requested,
                alias,
                mappings,
            })
        }
        ResolvedModel::PinnedRaw { requested, mapping } if !mapping.is_custom_api() => {
            Err(ResolveError::Ambiguous {
                requested,
                mappings: vec![mapping, custom_mapping(CUSTOM_DYNAMIC_UPSTREAM)],
            })
        }
        other => Ok(other),
    }
}

fn provider_catalog_hit_for_resolved(
    model_ids: &[String],
    resolved: &ResolvedModel,
    owns_mapping: impl Fn(&ProviderMapping) -> bool,
    alias_for_model: fn(&str) -> String,
) -> Option<String> {
    provider_catalog_hit(model_ids, resolved.requested()).or_else(|| match resolved {
        ResolvedModel::Alias {
            alias, mappings, ..
        } => provider_catalog_hit_by_alias(model_ids, alias, alias_for_model).or_else(|| {
            mappings
                .iter()
                .filter(|mapping| owns_mapping(mapping))
                .find_map(|mapping| provider_catalog_hit(model_ids, &mapping.upstream_model))
        }),
        ResolvedModel::PinnedRaw { mapping, .. } if owns_mapping(mapping) => {
            provider_catalog_hit(model_ids, &mapping.upstream_model)
        }
        _ => None,
    })
}

fn provider_catalog_hit_by_alias(
    model_ids: &[String],
    alias: &str,
    alias_for_model: fn(&str) -> String,
) -> Option<String> {
    model_ids
        .iter()
        .find(|id| !looks_raw_shaped(id) && alias_for_model(id).eq_ignore_ascii_case(alias))
        .cloned()
}

fn provider_catalog_hit(model_ids: &[String], requested: &str) -> Option<String> {
    model_ids
        .iter()
        .find(|id| id.trim() == requested.trim())
        .cloned()
}

fn overlay_unknown_provider(
    requested: String,
    model_ids: &[String],
    mapping: impl Fn(&str) -> ProviderMapping,
) -> Result<ResolvedModel, ResolveError> {
    let Some(canonical) = provider_catalog_hit(model_ids, &requested) else {
        return Err(ResolveError::Unknown { requested });
    };
    Ok(ResolvedModel::PinnedRaw {
        requested,
        mapping: mapping(&canonical),
    })
}

fn overlay_known_provider(
    resolved: ResolvedModel,
    canonical: String,
    mapping: impl Fn(&str) -> ProviderMapping,
) -> Result<ResolvedModel, ResolveError> {
    match resolved {
        ResolvedModel::Alias {
            requested,
            alias,
            mut mappings,
        } => {
            if let Some(existing) = mappings.iter_mut().find(|existing| {
                let replacement = mapping(&canonical);
                existing.provider_id == replacement.provider_id
            }) {
                *existing = mapping(&canonical);
            } else {
                mappings.push(mapping(&canonical));
            }
            Ok(ResolvedModel::Alias {
                requested,
                alias,
                mappings,
            })
        }
        ResolvedModel::PinnedRaw {
            requested,
            mapping: existing,
        } => {
            let replacement = mapping(&canonical);
            if existing.provider_id == replacement.provider_id {
                Ok(ResolvedModel::PinnedRaw {
                    requested,
                    mapping: replacement,
                })
            } else {
                Err(ResolveError::Ambiguous {
                    requested,
                    mappings: vec![existing, replacement],
                })
            }
        }
    }
}

fn overlay_goat_catalog(
    resolved: ResolvedModel,
    goat_model_ids: &[String],
    registry: &Registry,
) -> Result<ResolvedModel, ResolveError> {
    let Some(canonical) = goat_catalog_hit_for_resolved(goat_model_ids, &resolved, registry) else {
        return Ok(match resolved {
            ResolvedModel::PinnedRaw { requested, mapping }
                if mapping.is_command_code_goat() && mapping.routeable =>
            {
                ResolvedModel::PinnedRaw {
                    requested,
                    mapping: goat_mapping(&mapping.upstream_model, false),
                }
            }
            other => other,
        });
    };
    overlay_known_goat(resolved, canonical)
}

fn overlay_unknown_goat(
    requested: String,
    goat_model_ids: &[String],
    registry: &Registry,
) -> Result<ResolvedModel, ResolveError> {
    let Some(canonical) = goat_catalog_hit(goat_model_ids, &requested).or_else(|| {
        command_catalog_hit_for_alias(goat_model_ids, &requested.to_ascii_lowercase(), registry)
    }) else {
        return Err(ResolveError::Unknown { requested });
    };
    let alias = command_alias_for_catalog(&canonical, registry);
    if looks_raw_shaped(&requested)
        || (canonical.trim() == requested.trim() && !alias.eq_ignore_ascii_case(&requested))
    {
        return Ok(ResolvedModel::PinnedRaw {
            requested,
            mapping: goat_mapping(&canonical, true),
        });
    }
    if let Some(entry) = registry.aliases.get(&alias) {
        let mut mappings = entry.mappings.clone();
        if !mappings.iter().any(ProviderMapping::is_command_code_goat) {
            mappings.push(goat_mapping(&canonical, true));
        }
        return Ok(ResolvedModel::Alias {
            requested,
            alias: entry.alias.clone(),
            mappings,
        });
    }
    Ok(ResolvedModel::PinnedRaw {
        requested,
        mapping: goat_mapping(&canonical, true),
    })
}

fn overlay_known_goat(
    resolved: ResolvedModel,
    canonical: String,
) -> Result<ResolvedModel, ResolveError> {
    match resolved {
        ResolvedModel::Alias {
            requested,
            alias,
            mut mappings,
        } => {
            if let Some(existing) = mappings
                .iter_mut()
                .find(|mapping| mapping.is_command_code_goat())
            {
                existing.routeable = true;
                existing.upstream_model = canonical;
            } else {
                mappings.push(goat_mapping(&canonical, true));
            }
            Ok(ResolvedModel::Alias {
                requested,
                alias,
                mappings,
            })
        }
        ResolvedModel::PinnedRaw { requested, mapping } => {
            if mapping.is_command_code_goat() {
                return Ok(ResolvedModel::PinnedRaw {
                    requested,
                    mapping: goat_mapping(&canonical, true),
                });
            }
            Err(ResolveError::Ambiguous {
                requested,
                mappings: vec![mapping, goat_mapping(&canonical, true)],
            })
        }
    }
}

fn resolve_in(registry: &Registry, requested: &str) -> Result<ResolvedModel, ResolveError> {
    let original = requested.to_string();
    let trimmed = requested.trim();
    if trimmed.is_empty() {
        return Err(ResolveError::Unknown {
            requested: original,
        });
    }

    // Raw-looking IDs are exact provider identifiers. Case folding belongs to
    // published aliases (and the separate Custom matcher), never built-in raw
    // pins.
    if looks_raw_shaped(trimmed) {
        if let Some(mappings) = registry.raw_exact.get(trimmed) {
            return pin_or_ambiguous(original, mappings);
        }
        return Err(ResolveError::Unknown {
            requested: original,
        });
    }

    let folded = trimmed.to_lowercase();
    if registry
        .aliases
        .get(&folded)
        .is_some_and(|entry| entry.alias != trimmed)
    {
        if let Some(mappings) = registry.raw_exact.get(trimmed) {
            return pin_or_ambiguous(original, mappings);
        }
    }
    if let Some(entry) = registry.aliases.get(&folded) {
        return Ok(ResolvedModel::Alias {
            requested: original,
            alias: entry.alias.clone(),
            mappings: entry.mappings.clone(),
        });
    }
    if let Some(mappings) = registry.raw_exact.get(trimmed) {
        return pin_or_ambiguous(original, mappings);
    }
    Err(ResolveError::Unknown {
        requested: original,
    })
}

/// Preferred aliases present in the registry, including fail-closed-only names.
/// Client `GET /v1/models` uses [`published_routeable_aliases`] instead.
pub fn published_aliases() -> Vec<String> {
    registry()
        .aliases
        .values()
        .map(|entry| entry.alias.clone())
        .collect()
}

/// A routeable preferred alias advertised by `GET /v1/models`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishedAlias {
    pub alias: String,
    pub owned_by: String,
}

/// Routeable preferred aliases that `GET /v1/models` exposes, in deterministic
/// registry order. `owned_by` is the first routeable mapping's `provider_id`.
/// Non-routeable GOAT / Custom mappings stay unpublished.
///
/// First-wins `owned_by` is only the client list advertisement. Catalog and
/// application-model discovery use [`routeable_aliases_for`], which keeps an
/// alias under every provider that currently has a routeable mapping.
pub fn published_routeable_aliases() -> Vec<PublishedAlias> {
    published_routeable_in(registry())
}

pub fn published_routeable_aliases_with_zen(zen_free_models: &[String]) -> Vec<PublishedAlias> {
    published_routeable_in(&build_registry(zen_free_models))
}

/// Routeable aliases authorized by the original OpenCode Go table and sealed
/// Provider adapter maps. Refreshed catalogs may add mappings and raw pins, but never
/// publish names outside those code-owned maps.
pub fn published_routeable_aliases_with_catalogs(
    zen_free_models: &[String],
    goat_model_ids: &[String],
) -> Vec<PublishedAlias> {
    published_routeable_aliases_with_all_catalogs(&[], zen_free_models, goat_model_ids)
}

pub fn published_routeable_aliases_with_all_catalogs(
    go_model_ids: &[String],
    zen_free_models: &[String],
    goat_model_ids: &[String],
) -> Vec<PublishedAlias> {
    published_routeable_aliases_with_extended_catalogs(
        go_model_ids,
        zen_free_models,
        goat_model_ids,
        &[],
        &[],
    )
}

pub fn published_routeable_aliases_with_extended_catalogs(
    go_model_ids: &[String],
    zen_free_models: &[String],
    goat_model_ids: &[String],
    minimax_model_ids: &[String],
    kimi_model_ids: &[String],
) -> Vec<PublishedAlias> {
    published_routeable_aliases_with_runtime_catalogs(RuntimeCatalogs {
        go: go_model_ids,
        zen_free: zen_free_models,
        custom: &[],
        command_code: goat_model_ids,
        minimax: minimax_model_ids,
        kimi: kimi_model_ids,
        cpa: &[],
        extra: &[],
    })
}

/// Published code-owned aliases after applying all runtime catalogs. Exact
/// raw-only rows remain outside this Alias-only list.
pub fn published_routeable_aliases_with_runtime_catalogs(
    catalogs: RuntimeCatalogs<'_>,
) -> Vec<PublishedAlias> {
    published_routeable_in(&build_runtime_registry(catalogs))
}

fn go_catalog_alias(model_id: &str) -> String {
    model_id
        .trim()
        .to_ascii_lowercase()
        .replace(['_', ' '], "-")
}

fn sealed_catalog_alias(
    model_id: &str,
    aliases: &'static [(&'static str, &'static str)],
) -> Option<&'static str> {
    let trimmed = model_id.trim();
    aliases
        .iter()
        .find_map(|(upstream, alias)| (*upstream == trimmed).then_some(*alias))
}

fn minimax_catalog_alias(model_id: &str) -> Option<&'static str> {
    sealed_catalog_alias(model_id, MINIMAX_CN_ALIASES)
}

fn kimi_catalog_alias(model_id: &str) -> Option<&'static str> {
    sealed_catalog_alias(model_id, KIMI_CN_ALIASES)
}

fn command_catalog_alias(model_id: &str) -> Option<&'static str> {
    sealed_catalog_alias(model_id, COMMAND_CODE_GOAT_ALIASES)
}

fn is_code_owned_alias(registry: &Registry, alias: &str) -> bool {
    code_owned_alias(registry, alias).is_some()
}

/// Canonical spelling for a code-owned client alias. This remains an alias
/// authority check, not catalog-name normalization: CPA rows such as
/// `MiniMax-M3` stay exact raw pins unless CPA itself exposes `minimax-m3`.
fn code_owned_alias(registry: &Registry, candidate: &str) -> Option<String> {
    let candidate = candidate.trim();
    if looks_raw_shaped(candidate) {
        return None;
    }
    registry
        .aliases
        .get(&candidate.to_ascii_lowercase())
        .map(|entry| entry.alias.clone())
        .or_else(|| {
            MINIMAX_CN_ALIASES
                .iter()
                .chain(KIMI_CN_ALIASES)
                .chain(COMMAND_CODE_GOAT_ALIASES)
                .find_map(|(_, alias)| {
                    alias
                        .eq_ignore_ascii_case(candidate)
                        .then(|| (*alias).to_string())
                })
        })
}

fn command_alias_for_catalog(upstream_model: &str, registry: &Registry) -> String {
    let leaf = upstream_model
        .trim()
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
        .replace(['_', ' '], "-");
    let is_authorized_alias = |candidate: &str| is_code_owned_alias(registry, candidate);
    if is_authorized_alias(&leaf) {
        return leaf;
    }
    if let Some(alias) = command_catalog_alias(upstream_model) {
        return alias.to_string();
    }
    for (command_leaf, go_alias) in COMMAND_ALIAS_EXACT_EXCEPTIONS {
        if leaf.eq_ignore_ascii_case(command_leaf) && is_authorized_alias(go_alias) {
            return (*go_alias).to_string();
        }
    }
    for suffix in COMMAND_ALIAS_SUFFIX_EXCEPTIONS {
        if let Some(candidate) = leaf.strip_suffix(suffix) {
            if is_authorized_alias(candidate) {
                return candidate.to_string();
            }
        }
    }
    leaf
}

/// Canonical client Alias for one Provider catalog row. The original OpenCode
/// Go table and sealed Provider adapter maps are authoritative; an empty result means
/// the row is raw-only.
pub fn canonical_alias_for_provider_model(
    provider_id: &str,
    upstream_model: &str,
    _go_model_ids: &[String],
    zen_free_models: &[String],
) -> String {
    let registry = build_registry(zen_free_models);
    if provider_id == OPENCODE_PROVIDER_ID {
        let candidate = upstream_model.trim().to_ascii_lowercase();
        return registry
            .aliases
            .get(&candidate)
            .map(|entry| entry.alias.clone())
            .unwrap_or_default();
    }
    if provider_id == OPENCODE_ZEN_FREE_PROVIDER_ID {
        let candidate = stripped_free_alias(upstream_model)
            .unwrap_or(upstream_model)
            .trim()
            .to_ascii_lowercase();
        return registry
            .aliases
            .get(&candidate)
            .map(|entry| entry.alias.clone())
            .unwrap_or_default();
    }
    if provider_id == COMMAND_CODE_PROVIDER_ID {
        let candidate = command_alias_for_catalog(upstream_model, &registry);
        return if is_code_owned_alias(&registry, &candidate) {
            candidate
        } else {
            String::new()
        };
    }
    if provider_id == MINIMAX_PROVIDER_ID {
        return minimax_catalog_alias(upstream_model)
            .map(str::to_string)
            .unwrap_or_default();
    }
    if provider_id == KIMI_PROVIDER_ID {
        return kimi_catalog_alias(upstream_model)
            .map(str::to_string)
            .unwrap_or_default();
    }
    if provider_id == CUSTOM_PROVIDER_ID {
        return upstream_model.trim().to_string();
    }
    String::new()
}

/// Canonical client Alias for a CPA catalog row, if and only if the row is
/// already a code-owned alias. Other CPA rows are exact raw pins.
pub fn canonical_alias_for_cpa_model(upstream_model: &str) -> String {
    code_owned_alias(registry(), upstream_model).unwrap_or_default()
}

fn published_routeable_in(registry: &Registry) -> Vec<PublishedAlias> {
    registry
        .aliases
        .values()
        .filter_map(|entry| {
            entry
                .mappings
                .iter()
                .find(|mapping| mapping.routeable)
                .map(|mapping| PublishedAlias {
                    alias: entry.alias.clone(),
                    owned_by: mapping.provider_id.clone(),
                })
        })
        .collect()
}

/// Preferred aliases that currently have a routeable mapping for this
/// provider, in deterministic registry order. Raw upstream IDs are
/// never returned. Unroutable mappings yield an empty list without a
/// hardcoded per-plan alias set.
pub fn routeable_aliases_for(provider_id: &str) -> Vec<String> {
    routeable_aliases_for_in(registry(), provider_id)
}

fn routeable_aliases_for_in(registry: &Registry, provider_id: &str) -> Vec<String> {
    registry
        .aliases
        .values()
        .filter(|entry| {
            entry
                .mappings
                .iter()
                .any(|mapping| mapping.routeable && mapping.provider_id == provider_id)
        })
        .map(|entry| entry.alias.clone())
        .collect()
}

pub fn routeable_aliases_for_with_zen(
    provider_id: &str,
    zen_free_models: &[String],
) -> Vec<String> {
    routeable_aliases_for_in(&build_registry(zen_free_models), provider_id)
}

pub fn routeable_aliases_for_with_extended_catalogs(
    provider_id: &str,
    zen_free_models: &[String],
    goat_model_ids: &[String],
    minimax_model_ids: &[String],
    kimi_model_ids: &[String],
) -> Vec<String> {
    routeable_aliases_for_with_runtime_catalogs(
        provider_id,
        RuntimeCatalogs {
            go: &[],
            zen_free: zen_free_models,
            custom: &[],
            command_code: goat_model_ids,
            minimax: minimax_model_ids,
            kimi: kimi_model_ids,
            cpa: &[],
            extra: &[],
        },
    )
}

/// Routeable aliases for one sealed provider after applying all runtime
/// catalogs.
pub fn routeable_aliases_for_with_runtime_catalogs(
    provider_id: &str,
    catalogs: RuntimeCatalogs<'_>,
) -> Vec<String> {
    routeable_aliases_for_in(&build_runtime_registry(catalogs), provider_id)
}

pub fn is_published_alias(name: &str) -> bool {
    matches!(resolve(name), Ok(ResolvedModel::Alias { .. }))
}

type ResolveName = fn(&str) -> Result<ResolvedModel, ResolveError>;
type ResolveCustom = fn(&str, &[String]) -> Result<ResolvedModel, ResolveError>;
type ResolveProviderModels = fn(&str, &[String], &[String]) -> Result<ResolvedModel, ResolveError>;
type ResolveCatalogs =
    fn(&str, &[String], &[String], &[String]) -> Result<ResolvedModel, ResolveError>;
type RouteableWithZen = fn(&str, &[String]) -> Vec<String>;
type RouteableProviderExtendedCatalogs =
    fn(&str, &[String], &[String], &[String], &[String]) -> Vec<String>;
type ResolveRuntimeCatalogs =
    for<'a> fn(&str, RuntimeCatalogs<'a>) -> Result<ResolvedModel, ResolveError>;
type PublishRuntimeCatalogs = for<'a> fn(RuntimeCatalogs<'a>) -> Vec<PublishedAlias>;
type RouteableRuntimeCatalogs = for<'a> fn(&str, RuntimeCatalogs<'a>) -> Vec<String>;

const _: ResolveName = resolve;
const _: ResolveCustom = resolve_with_custom;
const _: ResolveProviderModels = resolve_with_provider_models;
const _: ResolveCatalogs = resolve_with_catalogs;
const _: ResolveRuntimeCatalogs = resolve_with_runtime_catalogs;
const _: fn() -> Vec<String> = published_aliases;
const _: fn() -> Vec<PublishedAlias> = published_routeable_aliases;
const _: fn(&[String]) -> Vec<PublishedAlias> = published_routeable_aliases_with_zen;
const _: fn(&[String], &[String]) -> Vec<PublishedAlias> =
    published_routeable_aliases_with_catalogs;
const _: fn(&str) -> Vec<String> = routeable_aliases_for;
const _: RouteableWithZen = routeable_aliases_for_with_zen;
const _: RouteableProviderExtendedCatalogs = routeable_aliases_for_with_extended_catalogs;
const _: PublishRuntimeCatalogs = published_routeable_aliases_with_runtime_catalogs;
const _: RouteableRuntimeCatalogs = routeable_aliases_for_with_runtime_catalogs;
const _: fn(&str) -> String = canonical_alias_for_cpa_model;
const _: fn(&str) -> bool = is_published_alias;

#[cfg(test)]
mod tests;
