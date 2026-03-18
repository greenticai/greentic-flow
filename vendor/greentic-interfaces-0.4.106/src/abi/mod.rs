#![cfg(all(feature = "bindings-rust", not(target_arch = "wasm32")))]

/// Versioned ABI facade for WIT `component@0.6.0`.
#[cfg(feature = "wit-v0_6_0")]
pub mod v0_6_0 {
    /// Canonical shared types used by helpers and examples.
    pub use crate::bindings::greentic_interfaces_pack_0_1_0_component::greentic::interfaces_types::types;

    /// v0.6 core types used by the node world.
    pub use crate::bindings::greentic_component_0_6_0_component::greentic::types_core::core;

    /// v0.6 component node interface.
    pub use crate::bindings::greentic_component_0_6_0_component::exports::greentic::component::node;

    /// Provider metadata contracts (kept versioned; not part of canonical surface yet).
    pub use crate::bindings::greentic_interfaces_pack_0_1_0_component::greentic::interfaces_provider::provider;

    /// Descriptor-centric contracts consumed by helper code and examples.
    pub use node::{
        ComponentDescriptor, IoSchema, Op, SchemaRef, SchemaSource, SetupContract, SetupExample,
        SetupOutput, SetupTemplateScaffold,
    };
}

/// Canonical ABI surface for this crate.
///
/// Today this resolves to `v0_6_0`; future releases can retarget it.
#[cfg(feature = "wit-v0_6_0")]
pub mod canonical {
    pub use super::v0_6_0::{
        ComponentDescriptor, IoSchema, Op, SchemaRef, SchemaSource, SetupContract, SetupExample,
        SetupOutput, SetupTemplateScaffold,
    };
    pub use super::v0_6_0::{core, node, types};
}
