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
    ANONYMOUS_FREE_OFFERING_ID, COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_ALIAS,
    COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM, COMMAND_CODE_PROVIDER_ID, CPA_OFFERING_ID,
    CPA_PROVIDER_ID, CUSTOM_API_OFFERING_ID, CUSTOM_PROVIDER_ID, GO_OFFERING_ID, GOAT_OFFERING_ID,
    KIMI_CN_OFFERING_ID, KIMI_PROVIDER_ID, MINIMAX_CN_OFFERING_ID, MINIMAX_PROVIDER_ID,
    OPENCODE_PROVIDER_ID, OPENCODE_ZEN_FREE_PROVIDER_ID, custom_model_id_matches, is_free_model,
    looks_raw_shaped,
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
    pub provider_id: &'static str,
    pub offering_id: &'static str,
    pub upstream_model: String,
    /// Production-routeable mappings only. Reserved offerings stay false.
    pub routeable: bool,
}

/// Borrowed runtime catalog inputs used to overlay the sealed Alias registry.
///
/// Callers pass one value instead of extending resolver signatures whenever a
/// new static adapter contributes a catalog. The registry remains code-owned;
/// this is data input, not a plugin or dynamic Alias authority.
#[derive(Debug, Clone, Copy, Default)]
pub struct RuntimeCatalogs<'a> {
    pub go: &'a [String],
    pub zen_free: &'a [String],
    pub custom: &'a [String],
    pub command_code: &'a [String],
    pub minimax: &'a [String],
    pub kimi: &'a [String],
    pub cpa: &'a [String],
}

impl ProviderMapping {
    pub fn is_opencode_go(&self) -> bool {
        self.provider_id == OPENCODE_PROVIDER_ID && self.offering_id == GO_OFFERING_ID
    }

    pub fn is_zen_free(&self) -> bool {
        self.provider_id == OPENCODE_ZEN_FREE_PROVIDER_ID
            && self.offering_id == ANONYMOUS_FREE_OFFERING_ID
    }

    pub fn is_command_code_goat(&self) -> bool {
        ocg_domain::provider::is_command_code_goat(self.provider_id, self.offering_id)
    }

    pub fn is_custom_api(&self) -> bool {
        is_custom_api(self.provider_id, self.offering_id)
    }

    pub fn is_minimax_cn(&self) -> bool {
        self.provider_id == MINIMAX_PROVIDER_ID && self.offering_id == MINIMAX_CN_OFFERING_ID
    }

    pub fn is_kimi_cn(&self) -> bool {
        self.provider_id == KIMI_PROVIDER_ID && self.offering_id == KIMI_CN_OFFERING_ID
    }

    pub fn is_cpa(&self) -> bool {
        self.provider_id == CPA_PROVIDER_ID && self.offering_id == CPA_OFFERING_ID
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
                    .map(|mapping| {
                        format!(
                            "{}/{}:{}",
                            mapping.provider_id, mapping.offering_id, mapping.upstream_model
                        )
                    })
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
    registry
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

fn go_mapping(upstream_model: &str) -> ProviderMapping {
    ProviderMapping {
        provider_id: OPENCODE_PROVIDER_ID,
        offering_id: GO_OFFERING_ID,
        upstream_model: upstream_model.to_string(),
        routeable: true,
    }
}

fn goat_deepseek_v4_flash_mapping() -> ProviderMapping {
    goat_mapping(COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM, false)
}

fn goat_mapping(upstream_model: &str, routeable: bool) -> ProviderMapping {
    ProviderMapping {
        provider_id: COMMAND_CODE_PROVIDER_ID,
        offering_id: GOAT_OFFERING_ID,
        upstream_model: upstream_model.to_string(),
        routeable,
    }
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
    ProviderMapping {
        provider_id: OPENCODE_ZEN_FREE_PROVIDER_ID,
        offering_id: ANONYMOUS_FREE_OFFERING_ID,
        upstream_model: upstream_model.to_string(),
        routeable: true,
    }
}

fn custom_mapping(upstream_model: &str) -> ProviderMapping {
    ProviderMapping {
        provider_id: CUSTOM_PROVIDER_ID,
        offering_id: CUSTOM_API_OFFERING_ID,
        upstream_model: upstream_model.to_string(),
        routeable: true,
    }
}

fn minimax_mapping(upstream_model: &str) -> ProviderMapping {
    ProviderMapping {
        provider_id: MINIMAX_PROVIDER_ID,
        offering_id: MINIMAX_CN_OFFERING_ID,
        upstream_model: upstream_model.to_string(),
        routeable: true,
    }
}

fn kimi_mapping(upstream_model: &str) -> ProviderMapping {
    ProviderMapping {
        provider_id: KIMI_PROVIDER_ID,
        offering_id: KIMI_CN_OFFERING_ID,
        upstream_model: upstream_model.to_string(),
        routeable: true,
    }
}

fn cpa_mapping(upstream_model: &str) -> ProviderMapping {
    ProviderMapping {
        provider_id: CPA_PROVIDER_ID,
        offering_id: CPA_OFFERING_ID,
        upstream_model: upstream_model.to_string(),
        routeable: true,
    }
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
            && existing.offering_id == mapping.offering_id
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
    match custom_resolved {
        Ok(resolved) => overlay_goat_catalog(resolved, catalogs.command_code, &registry),
        Err(ResolveError::Unknown { requested }) => {
            overlay_unknown_goat(requested, catalogs.command_code, &registry)
        }
        other => other,
    }
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
                    && existing.offering_id == replacement.offering_id
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
            if existing.provider_id == replacement.provider_id
                && existing.offering_id == replacement.offering_id
            {
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
    pub owned_by: &'static str,
}

/// Routeable preferred aliases that `GET /v1/models` exposes, in deterministic
/// registry order. `owned_by` is the first routeable mapping's `provider_id`.
/// Non-routeable GOAT / Custom mappings stay unpublished.
///
/// First-wins `owned_by` is only the client list advertisement. Catalog and
/// application-model discovery use [`routeable_aliases_for`], which keeps an
/// alias under every offering that currently has a routeable mapping.
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
                    owned_by: mapping.provider_id,
                })
        })
        .collect()
}

/// Preferred aliases that currently have a routeable mapping for this
/// provider/offering, in deterministic registry order. Raw upstream IDs are
/// never returned. Unroutable mappings (GOAT / Custom today) yield an
/// empty list without a hardcoded per-plan alias set.
pub fn routeable_aliases_for(provider_id: &str, offering_id: &str) -> Vec<String> {
    routeable_aliases_for_in(registry(), provider_id, offering_id)
}

fn routeable_aliases_for_in(
    registry: &Registry,
    provider_id: &str,
    offering_id: &str,
) -> Vec<String> {
    registry
        .aliases
        .values()
        .filter(|entry| {
            entry.mappings.iter().any(|mapping| {
                mapping.routeable
                    && mapping.provider_id == provider_id
                    && mapping.offering_id == offering_id
            })
        })
        .map(|entry| entry.alias.clone())
        .collect()
}

pub fn routeable_aliases_for_with_zen(
    provider_id: &str,
    offering_id: &str,
    zen_free_models: &[String],
) -> Vec<String> {
    routeable_aliases_for_in(&build_registry(zen_free_models), provider_id, offering_id)
}

pub fn routeable_aliases_for_with_extended_catalogs(
    provider_id: &str,
    offering_id: &str,
    zen_free_models: &[String],
    goat_model_ids: &[String],
    minimax_model_ids: &[String],
    kimi_model_ids: &[String],
) -> Vec<String> {
    routeable_aliases_for_with_runtime_catalogs(
        provider_id,
        offering_id,
        RuntimeCatalogs {
            go: &[],
            zen_free: zen_free_models,
            custom: &[],
            command_code: goat_model_ids,
            minimax: minimax_model_ids,
            kimi: kimi_model_ids,
            cpa: &[],
        },
    )
}

/// Routeable aliases for one sealed offering after applying all runtime
/// catalogs.
pub fn routeable_aliases_for_with_runtime_catalogs(
    provider_id: &str,
    offering_id: &str,
    catalogs: RuntimeCatalogs<'_>,
) -> Vec<String> {
    routeable_aliases_for_in(&build_runtime_registry(catalogs), provider_id, offering_id)
}

pub fn is_published_alias(name: &str) -> bool {
    matches!(resolve(name), Ok(ResolvedModel::Alias { .. }))
}

type ResolveName = fn(&str) -> Result<ResolvedModel, ResolveError>;
type ResolveCustom = fn(&str, &[String]) -> Result<ResolvedModel, ResolveError>;
type ResolveProviderModels = fn(&str, &[String], &[String]) -> Result<ResolvedModel, ResolveError>;
type ResolveCatalogs =
    fn(&str, &[String], &[String], &[String]) -> Result<ResolvedModel, ResolveError>;
type RouteableWithZen = fn(&str, &str, &[String]) -> Vec<String>;
type RouteableProviderExtendedCatalogs =
    fn(&str, &str, &[String], &[String], &[String], &[String]) -> Vec<String>;
type ResolveRuntimeCatalogs =
    for<'a> fn(&str, RuntimeCatalogs<'a>) -> Result<ResolvedModel, ResolveError>;
type PublishRuntimeCatalogs = for<'a> fn(RuntimeCatalogs<'a>) -> Vec<PublishedAlias>;
type RouteableRuntimeCatalogs = for<'a> fn(&str, &str, RuntimeCatalogs<'a>) -> Vec<String>;

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
const _: fn(&str, &str) -> Vec<String> = routeable_aliases_for;
const _: RouteableWithZen = routeable_aliases_for_with_zen;
const _: RouteableProviderExtendedCatalogs = routeable_aliases_for_with_extended_catalogs;
const _: PublishRuntimeCatalogs = published_routeable_aliases_with_runtime_catalogs;
const _: RouteableRuntimeCatalogs = routeable_aliases_for_with_runtime_catalogs;
const _: fn(&str) -> String = canonical_alias_for_cpa_model;
const _: fn(&str) -> bool = is_published_alias;

#[cfg(test)]
mod tests {
    use super::*;
    use std::any::{TypeId, type_name};

    fn production_source(source: &str) -> &str {
        source
            .split("#[cfg(test)]")
            .next()
            .expect("production source precedes tests")
    }

    fn seeded_free_models() -> Vec<String> {
        ZenFreeModelCatalog::default().models
    }

    #[test]
    fn alias_types_are_owned_by_this_module() {
        assert_eq!(
            type_name::<ProviderMapping>(),
            "ocg_gateway::alias::ProviderMapping"
        );
        assert_eq!(type_name::<AliasEntry>(), "ocg_gateway::alias::AliasEntry");
        assert_eq!(
            type_name::<ResolvedModel>(),
            "ocg_gateway::alias::ResolvedModel"
        );
        assert_eq!(
            type_name::<ResolveError>(),
            "ocg_gateway::alias::ResolveError"
        );
        assert_eq!(
            type_name::<PublishedAlias>(),
            "ocg_gateway::alias::PublishedAlias"
        );
        let _ = TypeId::of::<ProviderMapping>();
        let _ = TypeId::of::<AliasEntry>();
        let _ = TypeId::of::<ResolvedModel>();
        let _ = TypeId::of::<ResolveError>();
        let _ = TypeId::of::<PublishedAlias>();
        let _: ResolveName = resolve;
        let _: ResolveCustom = resolve_with_custom;
        let _: ResolveProviderModels = resolve_with_provider_models;
        let _: ResolveCatalogs = resolve_with_catalogs;
        let _: fn() -> Vec<String> = published_aliases;
        let _: fn() -> Vec<PublishedAlias> = published_routeable_aliases;
        let _: fn(&[String]) -> Vec<PublishedAlias> = published_routeable_aliases_with_zen;
        let _: fn(&str, &str) -> Vec<String> = routeable_aliases_for;
        let _: RouteableWithZen = routeable_aliases_for_with_zen;
        let _: fn(&str) -> bool = is_published_alias;
    }

    #[test]
    fn production_alias_source_stays_io_free_and_domain_only() {
        let production = production_source(include_str!("alias.rs"));
        assert!(
            production.contains("use ocg_domain::ids::{"),
            "alias.rs must import identities from ocg_domain::ids"
        );
        assert!(
            production.contains("use ocg_domain::protocol::supported_model_ids"),
            "alias.rs must import supported_model_ids from ocg_domain::protocol"
        );
        assert!(
            production.contains("use ocg_domain::provider::is_custom_api"),
            "alias.rs must import is_custom_api from ocg_domain::provider"
        );
        assert!(
            production.contains("use ocg_domain::zen::{"),
            "alias.rs must import Zen catalog helpers from ocg_domain::zen"
        );
        for needle in [
            "CoreState",
            "Database",
            "reqwest",
            "rusqlite",
            "tokio",
            "axum",
            "chrono",
            "ocg_core",
            "crate::custom",
            "std::fs",
            "std::net",
            "std::env",
            "std::process",
            "include!",
            "KeyCipher",
            "decrypt_key",
            "key_cipher",
        ] {
            assert!(
                !production.contains(needle),
                "production ocg-gateway alias source must not name `{needle}`"
            );
        }
    }

    #[test]
    fn go_model_ids_are_preferred_aliases() {
        let resolved = resolve("glm-5.2").expect("known Go model");
        match resolved {
            ResolvedModel::Alias {
                alias, mappings, ..
            } => {
                assert_eq!(alias, "glm-5.2");
                assert_eq!(mappings.len(), 1);
                assert!(mappings[0].is_opencode_go());
                assert!(mappings[0].routeable);
                assert_eq!(mappings[0].upstream_model, "glm-5.2");
            }
            other => panic!("expected alias, got {other:?}"),
        }
    }

    #[test]
    fn alias_lookup_is_case_insensitive_kebab() {
        let resolved = resolve("GLM-5.2").expect("case-folded alias");
        assert!(matches!(resolved, ResolvedModel::Alias { alias, .. } if alias == "glm-5.2"));
        assert!(is_published_alias("Grok-4.5"));
        assert!(is_published_alias(" glm-5.2 "));
        for alias in published_aliases() {
            assert_eq!(alias, alias.to_lowercase());
            assert!(!alias.is_empty());
            assert!(!looks_raw_shaped(&alias));
        }
    }

    #[test]
    fn raw_looking_names_do_not_collapse_onto_kebab_aliases() {
        for name in ["glm/5.2", "GLM_5.2", "Grok 4.5", "glm 5.2"] {
            match resolve(name) {
                Err(ResolveError::Unknown { requested }) => assert_eq!(requested, name),
                other => panic!("`{name}` must not collapse onto a kebab alias, got {other:?}"),
            }
            assert!(!is_published_alias(name));
        }
        assert!(matches!(
            resolve("glm-5.2").unwrap(),
            ResolvedModel::Alias { alias, .. } if alias == "glm-5.2"
        ));
    }

    #[test]
    fn free_ids_are_exact_pins_and_only_stripped_aliases_are_published() {
        let resolved = resolve("deepseek-v4-flash-free").expect("Zen model id");
        match resolved {
            ResolvedModel::PinnedRaw { mapping, .. } => {
                assert!(mapping.is_zen_free());
                assert_eq!(mapping.upstream_model, "deepseek-v4-flash-free");
            }
            other => panic!("expected raw pin, got {other:?}"),
        }
        assert!(matches!(
            resolve("deepseek-v4-flash"),
            Ok(ResolvedModel::Alias { mappings, .. }) if mappings.iter().any(ProviderMapping::is_zen_free)
        ));
        assert!(
            !published_aliases()
                .iter()
                .any(|alias| alias == "deepseek-v4-flash-free")
        );
    }

    #[test]
    fn shared_aliases_record_go_and_zen_mappings_in_the_registry() {
        match resolve("mimo-v2.5").unwrap() {
            ResolvedModel::Alias { mappings, .. } => {
                assert_eq!(mappings.len(), 2);
                assert!(mappings[0].is_opencode_go());
                assert!(mappings[1].is_zen_free());
                assert_eq!(mappings[1].upstream_model, "mimo-v2.5-free");
            }
            other => panic!("expected alias, got {other:?}"),
        }
        match resolve("glm-5.2").unwrap() {
            ResolvedModel::Alias { mappings, .. } => assert_eq!(mappings.len(), 1),
            other => panic!("expected alias, got {other:?}"),
        }
    }

    #[test]
    fn command_catalog_uses_go_canonical_aliases_and_keeps_raw_ids_pinned() {
        let go = vec!["hy3".to_string(), "future-go".to_string()];
        let zen = vec!["hy3-free".to_string()];
        let command = vec![
            "vendor/hy3".to_string(),
            "vendor/future-go".to_string(),
            "acme/Model_Name".to_string(),
            "stealth/ox-alpha".to_string(),
        ];

        match resolve_with_all_catalogs("hy3", &go, &zen, &[], &command).unwrap() {
            ResolvedModel::Alias {
                alias, mappings, ..
            } => {
                assert_eq!(alias, "hy3");
                assert!(mappings.iter().any(ProviderMapping::is_opencode_go));
                assert!(mappings.iter().any(ProviderMapping::is_zen_free));
                assert!(mappings.iter().any(|mapping| {
                    mapping.is_command_code_goat() && mapping.upstream_model == "vendor/hy3"
                }));
            }
            other => panic!("expected three-supplier Alias, got {other:?}"),
        }
        assert!(matches!(
            resolve_with_all_catalogs("vendor/hy3", &go, &zen, &[], &command),
            Ok(ResolvedModel::PinnedRaw { mapping, .. })
                if mapping.is_command_code_goat() && mapping.upstream_model == "vendor/hy3"
        ));
        assert!(matches!(
            resolve_with_all_catalogs("future-go", &go, &zen, &[], &command),
            Ok(ResolvedModel::PinnedRaw { mapping, .. })
                if mapping.is_opencode_go() && mapping.upstream_model == "future-go"
        ));
        let suffix_command = vec!["hy3-paid".to_string()];
        assert!(matches!(
            resolve_with_all_catalogs("hy3", &go, &zen, &[], &suffix_command),
            Ok(ResolvedModel::Alias { mappings, .. })
                if mappings.iter().any(|mapping| mapping.is_command_code_goat()
                    && mapping.upstream_model == "hy3-paid")
        ));
        assert!(matches!(
            resolve_with_all_catalogs("hy3-paid", &go, &zen, &[], &suffix_command),
            Ok(ResolvedModel::PinnedRaw { mapping, .. })
                if mapping.is_command_code_goat() && mapping.upstream_model == "hy3-paid"
        ));
        match resolve_with_all_catalogs("ox-alpha-free", &go, &zen, &[], &command).unwrap() {
            ResolvedModel::Alias { mappings, .. } => {
                assert!(mappings.iter().any(ProviderMapping::is_opencode_go));
                assert!(mappings.iter().any(|mapping| {
                    mapping.is_command_code_goat() && mapping.upstream_model == "stealth/ox-alpha"
                }));
            }
            other => panic!("expected Ox Alpha to share the Go baseline Alias, got {other:?}"),
        }
        assert!(matches!(
            resolve_with_all_catalogs("stealth/ox-alpha", &go, &zen, &[], &command),
            Ok(ResolvedModel::PinnedRaw { mapping, .. })
                if mapping.is_command_code_goat()
                    && mapping.upstream_model == "stealth/ox-alpha"
        ));

        match resolve_with_all_catalogs("model-name", &go, &zen, &[], &command).unwrap() {
            ResolvedModel::PinnedRaw { mapping, .. } => {
                assert!(mapping.is_command_code_goat());
                assert_eq!(mapping.upstream_model, "acme/Model_Name");
            }
            other => panic!("expected raw-only Command pin, got {other:?}"),
        }
        assert!(matches!(
            resolve_with_all_catalogs("acme/Model_Name", &go, &zen, &[], &command),
            Ok(ResolvedModel::PinnedRaw { mapping, .. })
                if mapping.is_command_code_goat() && mapping.upstream_model == "acme/Model_Name"
        ));

        let shared_raw = vec!["vendor/shared".to_string()];
        let error = resolve_with_all_catalogs("vendor/shared", &shared_raw, &[], &[], &shared_raw)
            .expect_err("provider raw collision must remain ambiguous");
        assert_eq!(error.code(), Some(AMBIGUOUS_MODEL_ID));

        let published = published_routeable_aliases_with_all_catalogs(&go, &zen, &command);
        assert_eq!(
            published.iter().filter(|item| item.alias == "hy3").count(),
            1
        );
        assert!(!published.iter().any(|item| item.alias == "model-name"));
        assert!(
            !published
                .iter()
                .any(|item| { item.alias.contains('/') || is_free_model(&item.alias) })
        );
    }

    #[test]
    fn command_catalog_shortens_only_code_owned_long_names() {
        let nemotron_upstream = COMMAND_CODE_GOAT_ALIASES[0].0.to_string();
        let command = vec![nemotron_upstream.clone()];

        match resolve_with_all_catalogs("nemotron-3-ultra", &[], &[], &[], &command).unwrap() {
            ResolvedModel::Alias {
                alias, mappings, ..
            } => {
                assert_eq!(alias, "nemotron-3-ultra");
                assert!(mappings.iter().any(|mapping| {
                    mapping.is_command_code_goat() && mapping.upstream_model == nemotron_upstream
                }));
            }
            other => panic!("expected sealed GOAT short Alias, got {other:?}"),
        }
        assert!(matches!(
            resolve_with_all_catalogs(&nemotron_upstream, &[], &[], &[], &command),
            Ok(ResolvedModel::PinnedRaw { mapping, .. })
                if mapping.is_command_code_goat()
                    && mapping.upstream_model == nemotron_upstream
        ));
        assert_eq!(
            canonical_alias_for_provider_model(
                COMMAND_CODE_PROVIDER_ID,
                &nemotron_upstream,
                &[],
                &[],
            ),
            "nemotron-3-ultra"
        );
        assert!(
            published_routeable_aliases_with_all_catalogs(&[], &[], &command)
                .iter()
                .any(|item| item.alias == "nemotron-3-ultra"
                    && item.owned_by == COMMAND_CODE_PROVIDER_ID)
        );
        assert_eq!(
            routeable_aliases_for_with_extended_catalogs(
                COMMAND_CODE_PROVIDER_ID,
                GOAT_OFFERING_ID,
                &[],
                &command,
                &[],
                &[],
            ),
            vec!["nemotron-3-ultra".to_string()]
        );
        assert!(
            !published_routeable_aliases_with_all_catalogs(&[], &[], &[])
                .iter()
                .any(|item| item.alias == "nemotron-3-ultra")
        );

        let future = vec!["vendor/future-model-with-a-very-long-name".to_string()];
        assert!(matches!(
            resolve_with_all_catalogs(
                "future-model-with-a-very-long-name",
                &[],
                &[],
                &[],
                &future,
            ),
            Ok(ResolvedModel::PinnedRaw { mapping, .. })
                if mapping.is_command_code_goat()
        ));
        assert!(
            !published_routeable_aliases_with_all_catalogs(&[], &[], &future)
                .iter()
                .any(|item| item.alias == "future-model-with-a-very-long-name")
        );
    }

    #[test]
    fn command_catalog_reuses_sealed_cn_aliases_and_known_plan_suffixes() {
        let command = vec![
            "moonshotai/Kimi-K2.7-Code-Highspeed".to_string(),
            "poolside/laguna-s-2.1-free".to_string(),
        ];
        for (alias, upstream) in [
            (
                "kimi-k2.7-code-highspeed",
                "moonshotai/Kimi-K2.7-Code-Highspeed",
            ),
            ("laguna-s-2.1", "poolside/laguna-s-2.1-free"),
        ] {
            match resolve_with_all_catalogs(
                alias,
                &[],
                &["laguna-s-2.1-free".into()],
                &[],
                &command,
            )
            .unwrap()
            {
                ResolvedModel::Alias { mappings, .. } => assert!(mappings.iter().any(|mapping| {
                    mapping.is_command_code_goat() && mapping.upstream_model == upstream
                })),
                other => panic!("expected code-owned Command Alias, got {other:?}"),
            }
        }
    }

    #[test]
    fn refreshed_zen_models_without_static_authority_stay_raw_only() {
        let models = vec!["brand-new-coder-free".to_string()];
        match resolve_with_provider_models("brand-new-coder-free", &models, &[]).unwrap() {
            ResolvedModel::PinnedRaw { mapping, .. } => {
                assert!(mapping.is_zen_free());
                assert_eq!(mapping.upstream_model, "brand-new-coder-free");
            }
            other => panic!("expected dynamic Zen raw pin, got {other:?}"),
        }
        assert!(matches!(
            resolve_with_provider_models("brand-new-coder", &models, &[]),
            Err(ResolveError::Unknown { .. })
        ));
        let published = published_routeable_aliases_with_zen(&models);
        assert!(
            !published
                .iter()
                .any(|entry| entry.alias == "brand-new-coder")
        );
        assert!(
            !published
                .iter()
                .any(|entry| entry.alias == "brand-new-coder-free")
        );
    }

    #[test]
    fn refreshed_catalog_cannot_steal_go_only_ox_alpha_free() {
        let models = vec!["ox-alpha-free".to_string()];
        match resolve_with_provider_models("ox-alpha-free", &models, &[]).unwrap() {
            ResolvedModel::Alias { mappings, .. } => {
                assert!(mappings.iter().all(ProviderMapping::is_opencode_go));
            }
            other => panic!("expected Go alias, got {other:?}"),
        }
        assert!(resolve_with_provider_models("ox-alpha", &models, &[]).is_err());
    }

    #[test]
    fn registry_covers_every_opencode_protocol_id() {
        let aliases = published_aliases();
        let free_models = seeded_free_models();
        for id in supported_model_ids() {
            if id == "big-pickle"
                || (is_free_model(id) && !free_models.iter().any(|free| free == id))
            {
                continue;
            }
            let expected_alias = if is_free_model(id) {
                stripped_free_alias(id).expect("free protocol id has a stripped alias")
            } else {
                id
            };
            assert!(
                aliases.iter().any(|alias| alias == expected_alias),
                "MODEL_PROTOCOLS id `{id}` must have an alias"
            );
        }
        for id in &free_models {
            let statically_authorized = supported_model_ids().any(|known| known == id);
            assert_eq!(
                stripped_free_alias(id)
                    .is_some_and(|alias| aliases.iter().any(|item| item == alias)),
                statically_authorized,
                "only a statically authorized free model may have a stripped alias: `{id}`"
            );
            assert!(resolve(id).unwrap().routeable_mappings()[0].is_zen_free());
        }
        assert!(!aliases.iter().any(|alias| alias.contains("goat")));
        assert!(
            !aliases
                .iter()
                .any(|alias| alias.contains("unknown-provider"))
        );
        assert!(!aliases.iter().any(|alias| alias.contains("custom")));
    }

    #[test]
    fn published_routeable_aliases_use_routeable_provider_ownership() {
        let published = published_routeable_aliases();
        assert!(!published.is_empty());
        let ids: Vec<&str> = published.iter().map(|item| item.alias.as_str()).collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        assert_eq!(ids, sorted, "GET /v1/models order must be deterministic");
        assert_eq!(
            published.len(),
            published_aliases().len(),
            "builtin aliases currently all have a routeable mapping"
        );
        for item in &published {
            assert!(!looks_raw_shaped(&item.alias));
            assert_ne!(item.alias, COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM);
            match resolve(&item.alias).unwrap() {
                ResolvedModel::Alias { mappings, .. } => {
                    let routeable = mappings
                        .iter()
                        .find(|mapping| mapping.routeable)
                        .expect("published alias must have a routeable mapping");
                    assert_eq!(item.owned_by, routeable.provider_id);
                    assert_ne!(item.owned_by, COMMAND_CODE_PROVIDER_ID);
                    assert_ne!(item.owned_by, "unknown-provider");
                    assert_ne!(item.owned_by, CUSTOM_PROVIDER_ID);
                }
                other => panic!("published id must be an alias, got {other:?}"),
            }
        }
        let go = published
            .iter()
            .find(|item| item.alias == "glm-5.2")
            .expect("Go alias");
        assert_eq!(go.owned_by, OPENCODE_PROVIDER_ID);
        assert!(
            !published
                .iter()
                .any(|item| item.alias == "deepseek-v4-flash-free")
        );
        let goat_alias = published
            .iter()
            .find(|item| item.alias == COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_ALIAS)
            .expect("Go still owns the kebab alias");
        assert_eq!(goat_alias.owned_by, OPENCODE_PROVIDER_ID);
        assert!(
            !published
                .iter()
                .any(|item| item.alias.contains('/') || is_free_model(&item.alias))
        );
    }

    #[test]
    fn slash_prefixed_goat_raw_pins_to_command_code_and_does_not_steal_go() {
        match resolve(COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM) {
            Ok(ResolvedModel::PinnedRaw { mapping, .. }) => {
                assert!(mapping.is_command_code_goat());
                assert!(!mapping.routeable);
                assert_eq!(
                    mapping.upstream_model,
                    COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM
                );
                assert!(
                    ResolvedModel::PinnedRaw {
                        requested: COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM.into(),
                        mapping: mapping.clone(),
                    }
                    .routeable_mappings()
                    .is_empty()
                );
            }
            other => panic!("GOAT raw id must uniquely pin to command-code/goat, got {other:?}"),
        }
        match resolve(COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_ALIAS).unwrap() {
            ResolvedModel::Alias {
                alias, mappings, ..
            } => {
                assert_eq!(alias, COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_ALIAS);
                assert!(mappings.iter().any(|mapping| mapping.is_opencode_go()));
                assert!(
                    mappings
                        .iter()
                        .any(|mapping| mapping.is_command_code_goat() && !mapping.routeable)
                );
                let routeable = mappings
                    .iter()
                    .filter(|mapping| mapping.routeable)
                    .collect::<Vec<_>>();
                assert_eq!(routeable.len(), 2);
                assert!(routeable.iter().any(|mapping| mapping.is_opencode_go()));
                assert!(routeable.iter().any(|mapping| mapping.is_zen_free()));
                assert_eq!(
                    routeable
                        .iter()
                        .find(|mapping| mapping.is_opencode_go())
                        .unwrap()
                        .upstream_model,
                    COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_ALIAS
                );
            }
            other => panic!("expected published Go alias, got {other:?}"),
        }
        assert!(is_published_alias(
            COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_ALIAS
        ));
        assert!(!is_published_alias(
            COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM
        ));
    }

    #[test]
    fn eligible_goat_catalog_joins_static_aliases_and_keeps_other_ids_raw() {
        let goat_ids = vec![
            COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM.to_string(),
            "claude-sonnet-4-6".into(),
        ];
        match resolve_with_catalogs(
            COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM,
            &[],
            &[],
            &goat_ids,
        )
        .unwrap()
        {
            ResolvedModel::PinnedRaw { mapping, .. } => {
                assert!(mapping.is_command_code_goat());
                assert!(mapping.routeable);
            }
            other => panic!("expected routeable GOAT pin, got {other:?}"),
        }
        match resolve_with_catalogs("claude-sonnet-4-6", &[], &[], &goat_ids).unwrap() {
            ResolvedModel::PinnedRaw { mapping, .. } => {
                assert!(mapping.is_command_code_goat());
                assert!(mapping.routeable);
            }
            other => panic!("expected raw-only GOAT pin, got {other:?}"),
        }
        match resolve_with_catalogs(
            COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_ALIAS,
            &[],
            &[],
            &goat_ids,
        )
        .unwrap()
        {
            ResolvedModel::Alias { mappings, .. } => {
                assert!(
                    mappings
                        .iter()
                        .any(|mapping| mapping.routeable && mapping.is_opencode_go())
                );
                assert!(
                    mappings
                        .iter()
                        .any(|mapping| mapping.routeable && mapping.is_command_code_goat())
                );
            }
            other => panic!("GOAT must not steal Go kebab alias, got {other:?}"),
        }
        let published = published_routeable_aliases_with_catalogs(&[], &goat_ids);
        assert!(
            !published
                .iter()
                .any(|item| item.alias == "claude-sonnet-4-6")
        );
        assert!(
            published
                .iter()
                .find(|item| item.alias == COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_ALIAS)
                .is_some_and(|item| item.owned_by == OPENCODE_PROVIDER_ID)
        );
        assert!(
            !published
                .iter()
                .any(|item| item.alias == COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM)
        );
        match resolve_with_catalogs(COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM, &[], &[], &[])
            .unwrap()
        {
            ResolvedModel::PinnedRaw { mapping, .. } => {
                assert!(mapping.is_command_code_goat());
                assert!(!mapping.routeable);
            }
            other => panic!("empty GOAT catalog must keep the static pin closed, got {other:?}"),
        }
        assert!(
            !published_aliases().iter().any(|alias| alias.contains('/')
                || *alias == COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM)
        );
    }

    #[test]
    fn unknown_names_are_not_aliases() {
        match resolve("definitely-not-a-model") {
            Err(ResolveError::Unknown { requested }) => {
                assert_eq!(requested, "definitely-not-a-model");
                assert!(
                    ResolveError::Unknown {
                        requested: requested.clone()
                    }
                    .code()
                    .is_none()
                );
            }
            other => panic!("expected unknown, got {other:?}"),
        }
    }

    #[test]
    fn unique_raw_id_pins_to_one_mapping() {
        let registry = registry_from_entries(vec![
            AliasEntry {
                alias: "widget".into(),
                mappings: vec![go_mapping("widget")],
            },
            AliasEntry {
                alias: "gadget".into(),
                mappings: vec![ProviderMapping {
                    provider_id: OPENCODE_PROVIDER_ID,
                    offering_id: GO_OFFERING_ID,
                    upstream_model: "vendor.gadget-v1".into(),
                    routeable: true,
                }],
            },
        ]);
        match resolve_in(&registry, "vendor.gadget-v1").unwrap() {
            ResolvedModel::PinnedRaw { mapping, .. } => {
                assert_eq!(mapping.upstream_model, "vendor.gadget-v1");
                assert!(mapping.is_opencode_go());
            }
            other => panic!("expected pinned raw, got {other:?}"),
        }
        // Alias still wins when a kebab string is both an alias and a raw ID.
        assert!(matches!(
            resolve_in(&registry, "widget").unwrap(),
            ResolvedModel::Alias { alias, .. } if alias == "widget"
        ));
        // Exact slash-form raw IDs pin without collapsing onto a kebab alias.
        let slash_registry = registry_from_entries(vec![AliasEntry {
            alias: "widget".into(),
            mappings: vec![ProviderMapping {
                provider_id: OPENCODE_PROVIDER_ID,
                offering_id: GO_OFFERING_ID,
                upstream_model: "vendor/widget-v1".into(),
                routeable: true,
            }],
        }]);
        match resolve_in(&slash_registry, "vendor/widget-v1").unwrap() {
            ResolvedModel::PinnedRaw { mapping, .. } => {
                assert_eq!(mapping.upstream_model, "vendor/widget-v1");
            }
            other => panic!("exact slash raw must pin, got {other:?}"),
        }
        assert!(matches!(
            resolve_in(&slash_registry, "Vendor/widget-v1"),
            Err(ResolveError::Unknown { .. })
        ));
        assert!(matches!(
            resolve_in(&slash_registry, "vendor-widget-v1"),
            Err(ResolveError::Unknown { .. })
        ));
    }

    #[test]
    fn overlapping_raw_ids_return_ambiguous_model_id() {
        let registry = registry_from_entries(vec![
            AliasEntry {
                alias: "alpha".into(),
                mappings: vec![go_mapping("shared-raw")],
            },
            AliasEntry {
                alias: "beta".into(),
                mappings: vec![zen_mapping("shared-raw")],
            },
        ]);
        match resolve_in(&registry, "shared-raw") {
            Err(error) => {
                assert_eq!(error.code(), Some(AMBIGUOUS_MODEL_ID));
                let message = error.message();
                assert!(message.contains(AMBIGUOUS_MODEL_ID));
                assert!(message.contains("shared-raw"));
                assert!(message.contains("opencode/go"));
                assert!(message.contains("opencode-zen-free"));
            }
            other => panic!("expected ambiguous, got {other:?}"),
        }
        // Preferred aliases still resolve even when their upstream IDs overlap.
        assert!(matches!(
            resolve_in(&registry, "alpha").unwrap(),
            ResolvedModel::Alias { alias, .. } if alias == "alpha"
        ));
    }

    #[test]
    fn fail_closed_raw_mapping_is_not_routeable() {
        let registry = registry_from_entries(vec![AliasEntry {
            alias: "visible".into(),
            mappings: vec![ProviderMapping {
                provider_id: "command-code",
                offering_id: "goat",
                upstream_model: "goat-only-raw".into(),
                routeable: false,
            }],
        }]);
        match resolve_in(&registry, "goat-only-raw") {
            Ok(ResolvedModel::PinnedRaw { mapping, .. }) => {
                assert!(!mapping.routeable);
                assert_eq!(mapping.provider_id, "command-code");
                assert!(
                    ResolvedModel::PinnedRaw {
                        requested: "goat-only-raw".into(),
                        mapping: mapping.clone(),
                    }
                    .routeable_mappings()
                    .is_empty()
                );
            }
            other => {
                panic!("fail-closed unique raw must pin without being routeable, got {other:?}")
            }
        }
        match resolve_in(&registry, "visible").unwrap() {
            ResolvedModel::Alias { mappings, .. } => {
                assert!(!mappings[0].routeable);
                assert!(
                    ResolvedModel::Alias {
                        requested: "visible".into(),
                        alias: "visible".into(),
                        mappings: mappings.clone(),
                    }
                    .routeable_mappings()
                    .is_empty()
                );
            }
            other => panic!("expected alias, got {other:?}"),
        }
        assert!(
            published_routeable_in(&registry).is_empty(),
            "fail-closed aliases must stay off GET /v1/models"
        );
    }

    #[test]
    fn catalog_aliases_are_routeable_mappings_in_registry_order() {
        let go = routeable_aliases_for(OPENCODE_PROVIDER_ID, GO_OFFERING_ID);
        let zen = routeable_aliases_for(OPENCODE_ZEN_FREE_PROVIDER_ID, ANONYMOUS_FREE_OFFERING_ID);
        let free_models = seeded_free_models();
        assert!(!go.is_empty());
        assert!(!zen.is_empty());
        let mut sorted_go = go.clone();
        sorted_go.sort_unstable();
        assert_eq!(go, sorted_go, "catalog aliases must be deterministic");
        let mut sorted_zen = zen.clone();
        sorted_zen.sort_unstable();
        assert_eq!(zen, sorted_zen);

        for alias in go.iter().chain(zen.iter()) {
            assert!(!looks_raw_shaped(alias));
            assert_ne!(*alias, COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM);
            assert!(!alias.contains('/'));
        }
        assert!(go.iter().any(|alias| alias == "glm-5.2"));
        assert!(
            go.iter()
                .any(|alias| alias == COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_ALIAS)
        );
        assert!(go.iter().any(|alias| alias == "minimax-m2.7-highspeed"));
        assert!(!go.iter().any(|alias| alias == "deepseek-v4-flash-free"));
        assert!(!zen.iter().any(|alias| alias == "glm-5.2"));
        assert!(zen.iter().any(|alias| alias == "deepseek-v4-flash"));
        assert!(!zen.iter().any(|alias| alias.ends_with("-free")));
        for id in free_models
            .iter()
            .filter(|id| supported_model_ids().any(|known| known == id.as_str()))
        {
            let alias = stripped_free_alias(id).expect("seeded Zen ids end in -free");
            assert!(
                zen.iter().any(|item| item == alias),
                "Zen catalog must include stripped `{id}` alias"
            );
            assert!(
                !go.iter().any(|item| item == id),
                "Go catalog must not include free `{id}`"
            );
        }
        for id in supported_model_ids().filter(|id| !is_free_model(id)) {
            if id != "big-pickle" {
                assert!(
                    go.iter().any(|alias| alias == id),
                    "Go catalog must include `{id}`"
                );
            }
            let has_free_twin = free_models
                .iter()
                .any(|free| stripped_free_alias(free).is_some_and(|alias| alias == id));
            assert_eq!(
                zen.iter().any(|alias| alias == id),
                has_free_twin,
                "Zen stripped aliases must match the refreshed Free catalog for `{id}`"
            );
        }

        assert!(routeable_aliases_for(COMMAND_CODE_PROVIDER_ID, GOAT_OFFERING_ID).is_empty());
        assert!(
            routeable_aliases_for(CUSTOM_PROVIDER_ID, CUSTOM_API_OFFERING_ID).is_empty(),
            "Custom catalog aliases stay empty; client IDs come from account capabilities"
        );
    }

    #[test]
    fn catalog_aliases_keep_every_routeable_offering_not_first_wins_owner() {
        let registry = registry_from_entries(vec![AliasEntry {
            alias: "shared".into(),
            mappings: vec![zen_mapping("shared"), go_mapping("shared")],
        }]);
        let published = published_routeable_in(&registry);
        assert_eq!(published.len(), 1);
        assert_eq!(published[0].alias, "shared");
        assert_eq!(
            published[0].owned_by, OPENCODE_ZEN_FREE_PROVIDER_ID,
            "GET /v1/models owned_by stays first-wins"
        );
        assert_eq!(
            routeable_aliases_for_in(&registry, OPENCODE_PROVIDER_ID, GO_OFFERING_ID),
            ["shared"]
        );
        assert_eq!(
            routeable_aliases_for_in(
                &registry,
                OPENCODE_ZEN_FREE_PROVIDER_ID,
                ANONYMOUS_FREE_OFFERING_ID
            ),
            ["shared"]
        );
        assert!(
            routeable_aliases_for_in(&registry, COMMAND_CODE_PROVIDER_ID, GOAT_OFFERING_ID)
                .is_empty()
        );
    }

    #[test]
    fn custom_overlay_does_not_steal_published_aliases_and_resolves_unknown_ids() {
        match resolve_with_custom("glm-5.2", &["glm-5.2".into()]).unwrap() {
            ResolvedModel::Alias {
                alias, mappings, ..
            } => {
                assert_eq!(alias, "glm-5.2");
                assert!(mappings.iter().any(|mapping| mapping.is_opencode_go()));
                assert!(mappings.iter().any(|mapping| mapping.is_custom_api()));
                let routeable = mappings
                    .iter()
                    .filter(|mapping| mapping.routeable)
                    .collect::<Vec<_>>();
                assert!(routeable.iter().any(|mapping| mapping.is_opencode_go()));
                assert!(routeable.iter().any(|mapping| mapping.is_custom_api()));
            }
            other => panic!("expected alias overlay, got {other:?}"),
        }
        match resolve_with_custom("my-local-model", &["my-local-model".into()]).unwrap() {
            ResolvedModel::Alias {
                alias, mappings, ..
            } => {
                assert_eq!(alias, "my-local-model");
                assert_eq!(mappings.len(), 1);
                assert!(mappings[0].is_custom_api());
                assert!(mappings[0].routeable);
            }
            other => panic!("expected custom-only alias, got {other:?}"),
        }
        match resolve_with_custom("org/model", &["org/model".into()]).unwrap() {
            ResolvedModel::Alias { alias, .. } => assert_eq!(alias, "org/model"),
            other => panic!("expected raw-shaped Custom public alias, got {other:?}"),
        }
        match resolve_with_custom(
            COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM,
            &[COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM.into()],
        ) {
            Err(error) => {
                assert_eq!(error.code(), Some(AMBIGUOUS_MODEL_ID));
                assert!(error.message().contains("command-code/goat"));
                assert!(error.message().contains("custom/api"));
            }
            other => panic!("GOAT raw overlapping Custom must be ambiguous, got {other:?}"),
        }
        assert!(matches!(
            resolve_with_custom("definitely-not-a-model", &[]),
            Err(ResolveError::Unknown { .. })
        ));
    }

    #[test]
    fn refreshed_go_catalog_adds_raw_pins_without_expanding_alias_authority() {
        let go = vec!["future-go-model".to_string(), "vendor/raw-go".to_string()];
        match resolve_with_all_catalogs("future-go-model", &go, &[], &[], &[]).unwrap() {
            ResolvedModel::PinnedRaw { mapping, .. } => {
                assert!(mapping.is_opencode_go());
                assert_eq!(mapping.upstream_model, "future-go-model");
            }
            other => panic!("expected dynamic raw Go pin, got {other:?}"),
        }
        match resolve_with_all_catalogs("vendor/raw-go", &go, &[], &[], &[]).unwrap() {
            ResolvedModel::PinnedRaw { mapping, .. } => {
                assert!(mapping.is_opencode_go());
                assert_eq!(mapping.upstream_model, "vendor/raw-go");
            }
            other => panic!("expected dynamic raw Go pin, got {other:?}"),
        }

        let published = published_routeable_aliases_with_all_catalogs(&go, &[], &[]);
        assert!(!published.iter().any(|item| item.alias == "future-go-model"));
        assert!(!published.iter().any(|item| item.alias == "vendor/raw-go"));

        let goat = vec!["vendor/raw-go".to_string()];
        let overlap = resolve_with_all_catalogs("vendor/raw-go", &go, &[], &[], &goat)
            .expect_err("overlapping raw provider IDs must remain ambiguous");
        assert_eq!(overlap.code(), Some(AMBIGUOUS_MODEL_ID));
    }

    #[test]
    fn sealed_cn_catalogs_join_static_aliases_and_preserve_raw_ambiguity() {
        let minimax = MINIMAX_CN_ALIASES
            .iter()
            .map(|(upstream, _)| (*upstream).to_string())
            .collect::<Vec<_>>();
        let kimi = KIMI_CN_ALIASES
            .iter()
            .map(|(upstream, _)| (*upstream).to_string())
            .collect::<Vec<_>>();

        for (upstream, alias) in MINIMAX_CN_ALIASES {
            let resolved =
                resolve_with_extended_catalogs(alias, &[], &[], &[], &[], &minimax, &kimi).unwrap();
            assert!(
                resolved.routeable_mappings().iter().any(|mapping| {
                    mapping.is_minimax_cn() && mapping.upstream_model == *upstream
                })
            );
            assert!(matches!(
                resolve_with_extended_catalogs(
                    upstream, &[], &[], &[], &[], &minimax, &kimi,
                ),
                Ok(ResolvedModel::PinnedRaw { mapping, .. })
                    if mapping.is_minimax_cn() && mapping.upstream_model == *upstream
            ));
            assert_eq!(
                canonical_alias_for_provider_model(MINIMAX_PROVIDER_ID, upstream, &[], &[]),
                *alias
            );
        }

        for (upstream, alias) in KIMI_CN_ALIASES {
            let resolved =
                resolve_with_extended_catalogs(alias, &[], &[], &[], &[], &minimax, &kimi).unwrap();
            assert!(
                resolved
                    .routeable_mappings()
                    .iter()
                    .any(|mapping| { mapping.is_kimi_cn() && mapping.upstream_model == *upstream })
            );
            assert!(matches!(
                resolve_with_extended_catalogs(
                    upstream, &[], &[], &[], &[], &minimax, &kimi,
                ),
                Ok(ResolvedModel::PinnedRaw { mapping, .. })
                    if mapping.is_kimi_cn() && mapping.upstream_model == *upstream
            ));
            assert_eq!(
                canonical_alias_for_provider_model(KIMI_PROVIDER_ID, upstream, &[], &[]),
                *alias
            );
        }

        let published =
            published_routeable_aliases_with_extended_catalogs(&[], &[], &[], &minimax, &kimi);
        for (_, alias) in MINIMAX_CN_ALIASES.iter().chain(KIMI_CN_ALIASES) {
            assert!(published.iter().any(|item| item.alias == *alias));
        }
        let mut expected_minimax = MINIMAX_CN_ALIASES
            .iter()
            .map(|(_, alias)| (*alias).to_string())
            .collect::<Vec<_>>();
        expected_minimax.sort();
        assert_eq!(
            routeable_aliases_for_with_extended_catalogs(
                MINIMAX_PROVIDER_ID,
                MINIMAX_CN_OFFERING_ID,
                &[],
                &[],
                &minimax,
                &kimi,
            ),
            expected_minimax
        );
        let mut expected_kimi = KIMI_CN_ALIASES
            .iter()
            .map(|(_, alias)| (*alias).to_string())
            .collect::<Vec<_>>();
        expected_kimi.sort();
        assert_eq!(
            routeable_aliases_for_with_extended_catalogs(
                KIMI_PROVIDER_ID,
                KIMI_CN_OFFERING_ID,
                &[],
                &[],
                &minimax,
                &kimi,
            ),
            expected_kimi
        );

        let without_m2 = minimax
            .iter()
            .filter(|model| model.as_str() != "MiniMax-M2")
            .cloned()
            .collect::<Vec<_>>();
        let published_without_m2 =
            published_routeable_aliases_with_extended_catalogs(&[], &[], &[], &without_m2, &kimi);
        assert!(
            !published_without_m2
                .iter()
                .any(|item| item.alias == "minimax-m2")
        );

        let unknown_minimax = vec!["MiniMax-Future".to_string()];
        assert!(matches!(
            resolve_with_extended_catalogs(
                "MiniMax-Future",
                &[],
                &[],
                &[],
                &[],
                &unknown_minimax,
                &[],
            ),
            Ok(ResolvedModel::PinnedRaw { mapping, .. }) if mapping.is_minimax_cn()
        ));
        assert!(matches!(
            resolve_with_extended_catalogs(
                "minimax-future",
                &[],
                &[],
                &[],
                &[],
                &unknown_minimax,
                &[],
            ),
            Err(ResolveError::Unknown { .. })
        ));
        assert_eq!(
            canonical_alias_for_provider_model(MINIMAX_PROVIDER_ID, "minimax-m3", &[], &[],),
            ""
        );

        let minimax_case = vec!["provider-case".to_string()];
        let kimi_case = vec!["PROVIDER-CASE".to_string()];
        assert!(matches!(
            resolve_with_extended_catalogs(
                "provider-case",
                &[],
                &[],
                &[],
                &[],
                &minimax_case,
                &kimi_case,
            ),
            Ok(ResolvedModel::PinnedRaw { mapping, .. }) if mapping.is_minimax_cn()
        ));
        assert!(matches!(
            resolve_with_extended_catalogs(
                "PROVIDER-CASE",
                &[],
                &[],
                &[],
                &[],
                &minimax_case,
                &kimi_case,
            ),
            Ok(ResolvedModel::PinnedRaw { mapping, .. }) if mapping.is_kimi_cn()
        ));

        let shared = vec!["vendor/shared".to_string()];
        let error =
            resolve_with_extended_catalogs("vendor/shared", &[], &[], &[], &[], &shared, &shared)
                .unwrap_err();
        assert_eq!(error.code(), Some(AMBIGUOUS_MODEL_ID));
    }

    #[test]
    fn cpa_catalog_joins_code_owned_aliases_and_keeps_raw_ids_exact_and_fail_closed() {
        let cpa = vec![
            "GLM-5.2".to_string(),
            "cpa-raw-model".to_string(),
            "vendor/cpa-raw".to_string(),
        ];
        let catalogs = RuntimeCatalogs {
            cpa: &cpa,
            ..RuntimeCatalogs::default()
        };
        match resolve_with_runtime_catalogs("glm-5.2", catalogs).unwrap() {
            ResolvedModel::Alias {
                alias, mappings, ..
            } => {
                assert_eq!(alias, "glm-5.2");
                assert!(mappings.iter().any(|mapping| {
                    mapping.is_cpa() && mapping.upstream_model == "GLM-5.2" && mapping.routeable
                }));
            }
            other => panic!("CPA code-owned Alias must join, got {other:?}"),
        }
        for raw in ["cpa-raw-model", "vendor/cpa-raw"] {
            match resolve_with_runtime_catalogs(raw, catalogs).unwrap() {
                ResolvedModel::PinnedRaw { mapping, .. } => {
                    assert!(mapping.is_cpa());
                    assert_eq!(mapping.upstream_model, raw);
                }
                other => panic!("CPA unknown catalog id must remain raw, got {other:?}"),
            }
        }
        assert_eq!(canonical_alias_for_cpa_model("GLM-5.2"), "glm-5.2");
        assert_eq!(canonical_alias_for_cpa_model("vendor/cpa-raw"), "");

        let published = published_routeable_aliases_with_runtime_catalogs(catalogs);
        assert!(published.iter().any(|item| item.alias == "glm-5.2"));
        assert!(!published.iter().any(|item| item.alias == "cpa-raw-model"));
        assert!(!published.iter().any(|item| item.alias == "vendor/cpa-raw"));
        assert_eq!(
            routeable_aliases_for_with_runtime_catalogs(CPA_PROVIDER_ID, CPA_OFFERING_ID, catalogs,),
            ["glm-5.2"]
        );

        let shared = vec!["vendor/shared".to_string()];
        let conflict = resolve_with_runtime_catalogs(
            "vendor/shared",
            RuntimeCatalogs {
                go: &shared,
                cpa: &shared,
                ..RuntimeCatalogs::default()
            },
        )
        .unwrap_err();
        assert_eq!(conflict.code(), Some(AMBIGUOUS_MODEL_ID));
    }
}
