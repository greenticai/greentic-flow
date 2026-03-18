#![forbid(unsafe_code)]

include!(concat!(env!("OUT_DIR"), "/gen_all_worlds.rs"));

pub mod host_helpers;

pub use host_helpers::{
    SecretsError, SecretsErrorV1_1, SecretsStoreHost, SecretsStoreHostV1_1, add_all_to_linker,
    add_secrets_store_compat_to_linker, add_secrets_store_to_linker,
    add_secrets_store_v1_1_to_linker,
    v1::{HostFns, add_all_v1_to_linker},
};
