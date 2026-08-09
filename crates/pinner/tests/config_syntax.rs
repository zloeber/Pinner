use std::fs;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn collect_files_with_extension(root: &Path, rel_dir: &str, ext: &str) -> Vec<PathBuf> {
    let dir = root.join(rel_dir);
    if !dir.exists() {
        return Vec::new();
    }

    let mut files = Vec::new();
    for entry in WalkDir::new(&dir).into_iter().flatten() {
        let path = entry.path();
        if path.is_file() && path.extension().is_some_and(|e| e == ext) {
            files.push(path.to_path_buf());
        }
    }
    files.sort();
    files
}

fn parse_toml(path: &Path) -> Result<(), String> {
    let body = fs::read_to_string(path).map_err(|e| format!("read error: {e}"))?;
    toml::from_str::<toml::Value>(&body).map_err(|e| e.to_string())?;
    Ok(())
}

fn parse_json(path: &Path) -> Result<(), String> {
    let body = fs::read_to_string(path).map_err(|e| format!("read error: {e}"))?;
    serde_json::from_str::<serde_json::Value>(&body).map_err(|e| e.to_string())?;
    Ok(())
}

fn parse_yaml(path: &Path) -> Result<(), String> {
    let body = fs::read_to_string(path).map_err(|e| format!("read error: {e}"))?;
    serde_yaml::from_str::<serde_yaml::Value>(&body).map_err(|e| e.to_string())?;
    Ok(())
}

#[test]
fn repository_config_files_have_valid_syntax() {
    let root = repo_root();

    let mut toml_files = vec![
        root.join("Cargo.toml"),
        root.join("book.toml"),
        root.join(".mise.toml"),
    ];
    toml_files.extend(collect_files_with_extension(&root, "crates", "toml"));

    let mut yaml_files = vec![root.join("Taskfile.yml"), root.join("Secretfile.yml")];
    yaml_files.extend(collect_files_with_extension(&root, "tasks", "yml"));
    yaml_files.extend(collect_files_with_extension(
        &root,
        ".github/workflows",
        "yml",
    ));

    let json_files = collect_files_with_extension(&root, "schemas", "json");

    let mut errors = Vec::new();

    for path in toml_files {
        if !path.exists() {
            errors.push(format!("missing TOML file: {}", path.display()));
            continue;
        }
        if let Err(err) = parse_toml(&path) {
            errors.push(format!("invalid TOML: {} ({err})", path.display()));
        }
    }

    for path in yaml_files {
        if !path.exists() {
            errors.push(format!("missing YAML file: {}", path.display()));
            continue;
        }
        if let Err(err) = parse_yaml(&path) {
            errors.push(format!("invalid YAML: {} ({err})", path.display()));
        }
    }

    for path in json_files {
        if let Err(err) = parse_json(&path) {
            errors.push(format!("invalid JSON: {} ({err})", path.display()));
        }
    }

    assert!(
        errors.is_empty(),
        "configuration syntax validation failed:\n{}",
        errors.join("\n")
    );
}
