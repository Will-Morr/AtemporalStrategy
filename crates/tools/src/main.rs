mod fixtures;
use atemporal_contracts::*;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
fn main() {
    if let Err(e) = run() {
        eprintln!("atemporal-tools: {e}");
        std::process::exit(1);
    }
}
fn run() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "help".into());
    let mut options = BTreeMap::new();
    while let Some(key) = args.next() {
        if !key.starts_with("--") {
            return Err(format!("expected --option, got {key}"));
        }
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for {key}"))?;
        if options.insert(key.clone(), value).is_some() {
            return Err(format!("duplicate option {key}"));
        }
    }
    let allowed: &[&str] = match command.as_str() {
        "schema" => &["--out"],
        "guide" => &["--content", "--prose", "--out", "--expected-hash"],
        "normalize" => &["--content", "--setup", "--out"],
        "fixtures" | "help" => &[],
        _ => return Err(format!("unknown command {command}; use help")),
    };
    for key in options.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(format!("unknown option {key}"));
        }
    }
    let option =
        |key: &str, default: &str| options.get(key).cloned().unwrap_or_else(|| default.into());
    match command.as_str() {
        "fixtures" => fixtures::generate()?,
        "schema" => {
            let schema = atemporal_contracts::json_schema();
            write(
                Path::new(&option("--out", "schemas/contracts-v2.json")),
                serde_json::to_vec_pretty(&schema).map_err(|e| e.to_string())?,
            )?;
        }
        "guide" => {
            let yaml = read(&option("--content", "config/content.yaml"))?;
            let content = if let Some(hash) = options.get("--expected-hash") {
                atemporal_content::load_archived_content(&yaml, hash)?
            } else {
                atemporal_content::load_content(&yaml)?
            };
            let prose = read(&option("--prose", "client/guide/introduction.html"))?;
            let out = option("--out", "target/guide");
            let manifest = atemporal_content::write_guide(&content, &prose, Path::new(&out))?;
            println!("{} {}", out, manifest.content_hash);
        }

        "normalize" => {
            let content = atemporal_content::load_content(&read(&option(
                "--content",
                "config/content.yaml",
            ))?)?;
            let setup =
                atemporal_content::load_setup(&read(&option("--setup", "config/game.yaml"))?)?;
            let out = PathBuf::from(option("--out", "target/normalized"));
            write(
                &out.join("content.json"),
                serde_json::to_vec_pretty(&content).map_err(|e| e.to_string())?,
            )?;
            write(
                &out.join("setup.json"),
                serde_json::to_vec_pretty(&setup).map_err(|e| e.to_string())?,
            )?;
            println!(
                "content_hash={}",
                atemporal_content::content_hash(&content)?
            );
        }
        _ => println!(
            "Commands: fixtures (regenerate authored golden data); schema [--out PATH]; guide [--content YAML --expected-hash HASH --prose HTML --out DIR]; normalize [--content YAML --setup YAML --out DIR]. Run from repository root; errors exit nonzero."
        ),
    }
    Ok(())
}
fn read(path: &str) -> Result<String> {
    fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))
}
fn write(path: &Path, bytes: Vec<u8>) -> Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(path, bytes).map_err(|e| e.to_string())
}
