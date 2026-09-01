//! Compatibility facade for [`ocg_gateway::alias`].
//!
//! Public items match the historical `ocg_core::alias` surface. Do not
//! glob-reexport or reexport the module itself. The owning I/O-free
//! registry lives in `ocg_gateway::alias`.
//!
//! Custom capability matching stays on `kernel::ids::custom_model_id_matches`;
//! the gateway implementation uses the equivalent domain matcher.

#[doc(inline)]
pub use ocg_gateway::alias::{
    AMBIGUOUS_MODEL_ID, AliasEntry, CUSTOM_DYNAMIC_UPSTREAM, ExtraProviderCatalog, ProviderMapping,
    PublishedAlias, ResolveError, ResolvedModel, RuntimeCatalogs, canonical_alias_for_cpa_model,
    canonical_alias_for_provider_model, is_published_alias, published_aliases,
    published_routeable_aliases, published_routeable_aliases_with_all_catalogs,
    published_routeable_aliases_with_extended_catalogs,
    published_routeable_aliases_with_runtime_catalogs, published_routeable_aliases_with_zen,
    resolve, resolve_with_all_catalogs, resolve_with_catalogs, resolve_with_custom,
    resolve_with_extended_catalogs, resolve_with_provider_models, resolve_with_runtime_catalogs,
    routeable_aliases_for, routeable_aliases_for_with_extended_catalogs,
    routeable_aliases_for_with_runtime_catalogs, routeable_aliases_for_with_zen,
};

type ResolveName = fn(&str) -> Result<ResolvedModel, ResolveError>;
type ResolveCustom = fn(&str, &[String]) -> Result<ResolvedModel, ResolveError>;
type ResolveProviderModels = fn(&str, &[String], &[String]) -> Result<ResolvedModel, ResolveError>;
type ResolveCatalogs =
    fn(&str, &[String], &[String], &[String]) -> Result<ResolvedModel, ResolveError>;
type ResolveAllCatalogs =
    fn(&str, &[String], &[String], &[String], &[String]) -> Result<ResolvedModel, ResolveError>;
type RouteableWithZen = fn(&str, &[String]) -> Vec<String>;
type RouteableProviderExtendedCatalogs =
    fn(&str, &[String], &[String], &[String], &[String]) -> Vec<String>;
type RouteableAllCatalogs = fn(&[String], &[String], &[String]) -> Vec<PublishedAlias>;
type ResolveExtendedCatalogs = fn(
    &str,
    &[String],
    &[String],
    &[String],
    &[String],
    &[String],
    &[String],
) -> Result<ResolvedModel, ResolveError>;
type RouteableExtendedCatalogs =
    fn(&[String], &[String], &[String], &[String], &[String]) -> Vec<PublishedAlias>;
type ResolveRuntimeCatalogs =
    for<'a> fn(&str, RuntimeCatalogs<'a>) -> Result<ResolvedModel, ResolveError>;
type PublishRuntimeCatalogs = for<'a> fn(RuntimeCatalogs<'a>) -> Vec<PublishedAlias>;
type RouteableRuntimeCatalogs = for<'a> fn(&str, RuntimeCatalogs<'a>) -> Vec<String>;

const _: ResolveName = resolve;
const _: ResolveName = ocg_gateway::alias::resolve;
const _: ResolveCustom = resolve_with_custom;
const _: ResolveProviderModels = resolve_with_provider_models;
const _: ResolveCatalogs = resolve_with_catalogs;
const _: ResolveAllCatalogs = resolve_with_all_catalogs;
const _: ResolveExtendedCatalogs = resolve_with_extended_catalogs;
const _: ResolveRuntimeCatalogs = resolve_with_runtime_catalogs;
const _: fn() -> Vec<String> = published_aliases;
const _: fn() -> Vec<PublishedAlias> = published_routeable_aliases;
const _: fn(&[String]) -> Vec<PublishedAlias> = published_routeable_aliases_with_zen;
const _: RouteableAllCatalogs = published_routeable_aliases_with_all_catalogs;
const _: RouteableExtendedCatalogs = published_routeable_aliases_with_extended_catalogs;
const _: PublishRuntimeCatalogs = published_routeable_aliases_with_runtime_catalogs;
const _: fn(&str) -> Vec<String> = routeable_aliases_for;
const _: RouteableWithZen = routeable_aliases_for_with_zen;
const _: RouteableProviderExtendedCatalogs = routeable_aliases_for_with_extended_catalogs;
const _: RouteableRuntimeCatalogs = routeable_aliases_for_with_runtime_catalogs;
const _: fn(&str) -> bool = is_published_alias;

#[cfg(test)]
mod tests {
    use super::*;
    use std::any::{TypeId, type_name};

    #[test]
    fn facade_reexports_gateway_alias_types_without_owning_them() {
        assert_eq!(
            TypeId::of::<ProviderMapping>(),
            TypeId::of::<ocg_gateway::alias::ProviderMapping>()
        );
        assert_eq!(
            TypeId::of::<AliasEntry>(),
            TypeId::of::<ocg_gateway::alias::AliasEntry>()
        );
        assert_eq!(
            TypeId::of::<ResolvedModel>(),
            TypeId::of::<ocg_gateway::alias::ResolvedModel>()
        );
        assert_eq!(
            TypeId::of::<ResolveError>(),
            TypeId::of::<ocg_gateway::alias::ResolveError>()
        );
        assert_eq!(
            TypeId::of::<PublishedAlias>(),
            TypeId::of::<ocg_gateway::alias::PublishedAlias>()
        );
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
        assert!(std::ptr::eq(
            AMBIGUOUS_MODEL_ID,
            ocg_gateway::alias::AMBIGUOUS_MODEL_ID
        ));
        assert!(std::ptr::eq(
            CUSTOM_DYNAMIC_UPSTREAM,
            ocg_gateway::alias::CUSTOM_DYNAMIC_UPSTREAM
        ));
        let _: ResolveName = resolve;
        let _: ResolveName = ocg_gateway::alias::resolve;
        let _: ResolveCustom = ocg_gateway::alias::resolve_with_custom;
        let _: ResolveProviderModels = ocg_gateway::alias::resolve_with_provider_models;
        let _: fn() -> Vec<String> = ocg_gateway::alias::published_aliases;
        let _: fn() -> Vec<PublishedAlias> = ocg_gateway::alias::published_routeable_aliases;
        let _: fn(&[String]) -> Vec<PublishedAlias> =
            ocg_gateway::alias::published_routeable_aliases_with_zen;
        let _: fn(&str) -> Vec<String> = ocg_gateway::alias::routeable_aliases_for;
        let _: RouteableWithZen = ocg_gateway::alias::routeable_aliases_for_with_zen;
        let _: RouteableProviderExtendedCatalogs =
            ocg_gateway::alias::routeable_aliases_for_with_extended_catalogs;
        let _: fn(&str) -> bool = ocg_gateway::alias::is_published_alias;
    }
}
