use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

pub const TOKEN_ENV: &str = "NOTION_TOKEN";

#[derive(Deserialize)]
struct ConfigFile {
    token: Option<String>,
}

/// Emplacement du fichier de configuration : `<config>/printnotion/config.toml`.
pub fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("printnotion").join("config.toml"))
}

/// Lit le token d'intégration Notion : variable d'environnement d'abord, fichier de config ensuite.
pub fn load_token() -> Result<String> {
    if let Ok(token) = std::env::var(TOKEN_ENV) {
        let token = token.trim();
        if !token.is_empty() {
            return Ok(token.to_owned());
        }
    }

    let path = config_path();
    if let Some(path) = path.as_ref().filter(|p| p.exists()) {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("Impossible de lire {}", path.display()))?;
        let config: ConfigFile = toml::from_str(&raw)
            .with_context(|| format!("Fichier de configuration invalide : {}", path.display()))?;
        if let Some(token) = config.token.map(|t| t.trim().to_owned()).filter(|t| !t.is_empty()) {
            return Ok(token);
        }
    }

    let path_hint = path
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<dossier de config>/printnotion/config.toml".into());
    bail!(
        "Aucun token Notion trouvé.\n\
         Créez une intégration sur https://www.notion.so/profile/integrations, puis :\n\
         \x20 - soit exportez la variable d'environnement : export {TOKEN_ENV}=ntn_...\n\
         \x20 - soit créez le fichier {path_hint} contenant : token = \"ntn_...\""
    )
}
