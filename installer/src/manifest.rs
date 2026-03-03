use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct ManifestTool {
    pub name: String,
    pub mise_id: String,
    pub description: String,
}

#[derive(Deserialize)]
struct Manifest {
    tool: Vec<ManifestTool>,
}

/// Load the curated tool list embedded at compile time.
pub fn load_manifest() -> Vec<ManifestTool> {
    let raw = include_str!("../mise_registry.toml");
    let m: Manifest = toml::from_str(raw).expect("mise_registry.toml is invalid TOML");
    m.tool
}

/// Case-insensitive substring search across name and description.
pub fn search(tools: &[ManifestTool], query: &str) -> Vec<ManifestTool> {
    if query.is_empty() {
        return tools.to_vec();
    }
    let q = query.to_lowercase();
    tools
        .iter()
        .filter(|t| {
            t.name.to_lowercase().contains(&q)
                || t.description.to_lowercase().contains(&q)
        })
        .cloned()
        .collect()
}

/// Runtime fallback: query `mise registry` if mise is already installed.
/// Merges results with the curated manifest (deduplicates by mise_id).
pub fn load_runtime_registry() -> Option<Vec<ManifestTool>> {
    let output = std::process::Command::new("mise")
        .args(["registry"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let tools: Vec<ManifestTool> = text
        .lines()
        .filter_map(|line| {
            // Lines look like: "toolname    backend:org/repo"
            let mut parts = line.split_whitespace();
            let name = parts.next()?.to_string();
            let mise_id = parts.next()?.to_string();
            Some(ManifestTool {
                description: mise_id.clone(),
                name,
                mise_id,
            })
        })
        .collect();

    if tools.is_empty() {
        None
    } else {
        Some(tools)
    }
}

/// Merge curated manifest with runtime registry, deduplicating by `mise_id`.
pub fn merge(curated: Vec<ManifestTool>, runtime: Vec<ManifestTool>) -> Vec<ManifestTool> {
    let mut seen: std::collections::HashSet<String> =
        curated.iter().map(|t| t.mise_id.clone()).collect();
    let mut merged = curated;
    for tool in runtime {
        if seen.insert(tool.mise_id.clone()) {
            merged.push(tool);
        }
    }
    merged
}
