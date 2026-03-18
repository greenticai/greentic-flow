use camino::Utf8PathBuf;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use std::collections::BTreeMap;
use std::{env, fs};
use walkdir::WalkDir;

fn world_names_from_str(content: &str) -> Vec<String> {
    content
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            if let Some(rest) = trimmed.strip_prefix("world ") {
                let name = rest
                    .split_whitespace()
                    .next()
                    .unwrap_or("world")
                    .trim_end_matches('{')
                    .to_string();
                return Some(name);
            }
            None
        })
        .collect()
}

fn module_name_from_dir_and_world(dir: &str, world: &str) -> String {
    let mut parts = dir.split('@');
    let raw_name = parts
        .next()
        .unwrap_or(dir)
        .replace(['-', '/', ':', '.'], "_");
    let version = parts.next().unwrap_or("0.0.0");
    let mut ver_parts = version.trim_start_matches('v').split('.');
    let major = ver_parts.next().unwrap_or("0");
    let minor = ver_parts.next().unwrap_or("0");
    let world_part = world.replace('-', "_");

    if world_part == raw_name {
        format!("{raw_name}_v{major}_{minor}")
    } else {
        format!("{raw_name}_{world_part}_v{major}_{minor}")
    }
}

fn main() {
    let out_dir = Utf8PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set"));
    let manifest_dir =
        Utf8PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set"));
    let canonical_root = Utf8PathBuf::from_path_buf(greentic_interfaces::wit_root())
        .expect("canonical WIT root must be valid UTF-8");
    assert!(
        canonical_root.is_dir(),
        "canonical WIT root not found at {} (expected from greentic_interfaces::wit_root())",
        canonical_root
    );
    let wit_root = canonical_root.join("greentic");
    let staged_root = manifest_dir.join("target").join("wit-staging-wasmtime");
    reset_directory(&staged_root);
    println!("cargo:rerun-if-changed={}", canonical_root);

    let package_catalog = build_package_catalog(&canonical_root);
    for (package_ref, package_file) in &package_catalog {
        stage_package(package_ref, package_file, &staged_root, &package_catalog);
    }

    let mut modules: Vec<TokenStream> = Vec::new();

    for entry in WalkDir::new(&wit_root) {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        if !entry.file_type().is_file() || entry.file_name() != "package.wit" {
            continue;
        }
        let package_path =
            Utf8PathBuf::from_path_buf(entry.path().to_path_buf()).expect("non-utf8 path");
        if package_path.components().any(|c| c.as_str() == "deps") {
            continue;
        }

        let package_dir = package_path
            .parent()
            .expect("package.wit must have a parent directory");
        let dirname = package_dir
            .file_name()
            .map(|n| n.to_string())
            .unwrap_or_default();

        let content =
            fs::read_to_string(&package_path).unwrap_or_else(|_| panic!("Reading {package_path}"));

        let package_line = content
            .lines()
            .find(|line| line.trim_start().starts_with("package "))
            .unwrap_or_else(|| panic!("package declaration not found in {package_path}"));
        let package_ref = package_line
            .trim_start()
            .trim_start_matches("package")
            .trim()
            .trim_end_matches(';')
            .trim();
        let (package_id, version) = package_ref
            .rsplit_once('@')
            .unwrap_or((package_ref, "0.0.0"));

        let world_wit = package_dir.join("world.wit");
        let mut world_names = world_names_from_str(&content);
        if world_wit.exists()
            && world_names.is_empty()
            && let Ok(extra) = fs::read_to_string(&world_wit)
        {
            world_names = world_names_from_str(&extra);
        }

        if world_names.is_empty() {
            continue;
        }

        world_names.sort();

        for world_name in world_names {
            let module_name = module_name_from_dir_and_world(&dirname, &world_name);
            let mod_ident = format_ident!("{}", module_name);
            let world_spec = format!("{package_id}/{world_name}@{version}");
            let staged_name = sanitize(&format!("{package_id}@{version}"));
            let package_rel_path = format!("{}/{}", staged_root, staged_name);

            let has_control_helpers = dirname.starts_with("component@")
                && content.contains("interface control")
                && content.contains("import control");

            let control_helpers = if has_control_helpers {
                quote! {
                    #[cfg(feature = "control-helpers")]
                    pub use bindings::greentic::component::control::Host as ControlHost;

                    #[cfg(feature = "control-helpers")]
                    pub use bindings::greentic::component::control::add_to_linker as add_control_to_linker;
                }
            } else {
                quote! {}
            };

            let module_tokens = quote! {
                pub mod #mod_ident {
                    mod bindings {
                        wasmtime::component::bindgen!({
                            path: #package_rel_path,
                            world: #world_spec
                        });
                    }

                    #[allow(unused_imports)]
                    pub use bindings::*;

                    /// Convenience shim to instantiate a component binary.
                    pub struct Component;
                    impl Component {
                        pub fn instantiate(
                            engine: &wasmtime::Engine,
                            component_wasm: &[u8],
                        ) -> anyhow::Result<wasmtime::component::Component> {
                            let component = wasmtime::component::Component::from_binary(engine, component_wasm)?;
                            Ok(component)
                        }
                    }

                    #control_helpers
                }
            };

            modules.push(module_tokens);
        }
    }

    modules.sort_by_key(|tokens| tokens.to_string());

    let src = quote! {
        // Auto-generated modules for each greentic WIT world discovered under canonical WIT.
        #(#modules)*
    };

    fs::create_dir_all(&out_dir).expect("create OUT_DIR");
    let gen_path = out_dir.join("gen_all_worlds.rs");
    fs::write(&gen_path, src.to_string()).expect("write generated bindings");
}

fn reset_directory(path: &Utf8PathBuf) {
    if path.exists() {
        fs::remove_dir_all(path).expect("failed to clear staging dir");
    }
    fs::create_dir_all(path).expect("failed to create staging dir");
}

fn sanitize(package_ref: &str) -> String {
    package_ref.replace([':', '@', '/'], "-")
}

fn build_package_catalog(wit_root: &Utf8PathBuf) -> BTreeMap<String, Utf8PathBuf> {
    let mut catalog = BTreeMap::new();
    for entry in WalkDir::new(wit_root) {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        if !entry.file_type().is_file() {
            continue;
        }
        let path = Utf8PathBuf::from_path_buf(entry.path().to_path_buf()).expect("non-utf8 path");
        if path.extension() != Some("wit") {
            continue;
        }
        if entry.path().components().any(|c| c.as_os_str() == "deps") {
            continue;
        }
        let content = fs::read_to_string(&path).unwrap_or_default();
        if let Some(line) = content
            .lines()
            .find(|l| l.trim_start().starts_with("package "))
        {
            let package_ref = line
                .trim_start()
                .trim_start_matches("package")
                .trim()
                .trim_end_matches(';')
                .trim()
                .to_string();
            catalog.entry(package_ref).or_insert(path);
        }
    }

    catalog
}

fn stage_package(
    package_ref: &str,
    package_file: &Utf8PathBuf,
    staged_root: &Utf8PathBuf,
    catalog: &BTreeMap<String, Utf8PathBuf>,
) {
    let dest_dir = staged_root.join(sanitize(package_ref));
    if dest_dir.exists() {
        return;
    }
    fs::create_dir_all(&dest_dir).expect("failed to create staged package dir");

    fs::copy(package_file, dest_dir.join("package.wit")).expect("failed to copy package.wit");

    // Copy helper .wit files that do not declare their own package (e.g. world.wit).
    let package_dir = package_file.parent().expect("package file parent");
    if let Ok(entries) = fs::read_dir(package_dir) {
        for entry in entries.flatten() {
            let path = Utf8PathBuf::from_path_buf(entry.path()).expect("non-utf8 path");
            if path.extension() != Some("wit") || path == *package_file {
                continue;
            }
            let content = fs::read_to_string(&path).unwrap_or_default();
            if content
                .lines()
                .any(|l| l.trim_start().starts_with("package "))
            {
                continue;
            }
            let dest = dest_dir.join(path.file_name().expect("wit filename"));
            fs::copy(&path, dest).expect("failed to copy helper wit");
        }
    }

    let deps = parse_deps(package_file);
    if deps.is_empty() {
        return;
    }

    let deps_dir = dest_dir.join("deps");
    fs::create_dir_all(&deps_dir).expect("failed to create deps dir");

    for dep_ref in deps {
        if let Some(dep_file) = catalog.get(&dep_ref) {
            stage_package(&dep_ref, dep_file, staged_root, catalog);
            let dep_dest = deps_dir.join(sanitize(&dep_ref));
            if !dep_dest.exists() {
                fs::create_dir_all(&dep_dest).expect("failed to create dep dest");
                let staged_dep = staged_root.join(sanitize(&dep_ref));
                copy_dir(&staged_dep, &dep_dest);
            }
        }
    }
}

fn parse_deps(package_file: &Utf8PathBuf) -> Vec<String> {
    let content = fs::read_to_string(package_file).unwrap_or_default();
    let mut deps = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim_start();
        let token = if trimmed.starts_with("use ") {
            trimmed.trim_start_matches("use ").trim()
        } else if trimmed.starts_with("import ") {
            trimmed.trim_start_matches("import ").trim()
        } else {
            continue;
        };
        let token = token
            .split(';')
            .next()
            .unwrap_or("")
            .split('{')
            .next()
            .unwrap_or("")
            .split(".{")
            .next()
            .unwrap_or("")
            .trim_end_matches('.');
        if !token.contains('@') {
            continue;
        }
        let pkg_with_world = token.split('@').next().unwrap_or("");
        let version = token.split('@').nth(1).unwrap_or("");
        let pkg = pkg_with_world.split('/').next().unwrap_or("");
        if !pkg.is_empty() && !version.is_empty() {
            deps.push(format!("{pkg}@{version}"));
        }
    }
    deps.sort();
    deps.dedup();
    deps
}

fn copy_dir(src: &Utf8PathBuf, dst: &Utf8PathBuf) {
    if let Ok(entries) = fs::read_dir(src) {
        for entry in entries.flatten() {
            let path = Utf8PathBuf::from_path_buf(entry.path()).expect("non-utf8 path");
            let dest = dst.join(path.file_name().expect("filename"));
            if path.is_dir() {
                fs::create_dir_all(&dest).expect("create dir");
                copy_dir(&path, &dest);
            } else {
                fs::copy(&path, &dest).expect("copy file");
            }
        }
    }
}
