//! Host runtime for persisted dynamic Provider definitions.
//!
//! Adapter identity is always the sealed Configurable HTTP adapter. Custom API
//! remains account-owned and is not a dynamic Provider.

use chrono::{DateTime, Utc};
use ocg_domain::dynamic::{
    DynamicAuthKind, DynamicModelMapping, DynamicProviderDefinition, normalize_dynamic_mappings,
    normalize_dynamic_provider_name,
};
use ocg_domain::provider::{BUILTIN_PROVIDERS, ProviderAdapterKind, ProviderBindingError};

pub use ocg_domain::dynamic::provider_ids_equal;

/// Frozen routing view of one dynamic Provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DynamicProviderRuntime {
    pub id: String,
    pub name: String,
    pub endpoint_url: String,
    pub upstream_protocol: ocg_domain::catalog::UpstreamProtocolKind,
    pub auth_kind: DynamicAuthKind,
    pub mappings: Vec<DynamicModelMapping>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl DynamicProviderRuntime {
    pub fn definition(&self) -> DynamicProviderDefinition {
        DynamicProviderDefinition {
            id: self.id.clone(),
            name: self.name.clone(),
            endpoint_url: self.endpoint_url.clone(),
            upstream_protocol: self.upstream_protocol,
            auth_kind: self.auth_kind,
            mappings: self.mappings.clone(),
        }
    }

    pub fn alias_catalog(&self) -> crate::alias::ExtraProviderCatalog {
        crate::alias::ExtraProviderCatalog {
            provider_id: self.id.clone(),
            mappings: self
                .mappings
                .iter()
                .map(|mapping| (mapping.public_model.clone(), mapping.upstream_model.clone()))
                .collect(),
        }
    }

    pub fn mapping_for_public(&self, requested: &str) -> Option<&DynamicModelMapping> {
        self.mappings.iter().find(|mapping| {
            crate::custom::custom_model_id_matches(&mapping.public_model, requested)
        })
    }

    pub fn mapping_for_upstream(&self, requested: &str) -> Option<&DynamicModelMapping> {
        self.mappings
            .iter()
            .find(|mapping| mapping.upstream_model.trim() == requested.trim())
    }
}

pub fn find_runtime<'a>(
    runtimes: &'a [DynamicProviderRuntime],
    provider_id: &str,
) -> Option<&'a DynamicProviderRuntime> {
    runtimes
        .iter()
        .find(|runtime| provider_ids_equal(&runtime.id, provider_id))
}

pub fn adapter_kind_for(
    provider_id: &str,
    runtimes: &[DynamicProviderRuntime],
) -> Option<ProviderAdapterKind> {
    if let Some(kind) = ProviderAdapterKind::from_provider_id(provider_id) {
        return Some(kind);
    }
    find_runtime(runtimes, provider_id).map(|_| ProviderAdapterKind::ConfigurableHttp)
}

pub fn collides_with_known_id(provider_id: &str, runtimes: &[DynamicProviderRuntime]) -> bool {
    BUILTIN_PROVIDERS
        .iter()
        .any(|plan| provider_ids_equal(plan.provider_id, provider_id))
        || find_runtime(runtimes, provider_id).is_some()
}

pub fn validate_definition(
    definition: DynamicProviderDefinition,
) -> Result<DynamicProviderDefinition, ProviderBindingError> {
    let name = normalize_dynamic_provider_name(&definition.name)?;
    let mappings = normalize_dynamic_mappings(&definition.mappings)?;
    Ok(DynamicProviderDefinition {
        id: definition.id,
        name,
        endpoint_url: definition.endpoint_url,
        upstream_protocol: definition.upstream_protocol,
        auth_kind: definition.auth_kind,
        mappings,
    })
}
