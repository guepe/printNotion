use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

pub const TOKEN_ENV: &str = "NOTION_TOKEN";

#[derive(Deserialize)]
struct ConfigFile {
    token: Option<String>,
    /// Titres de toggles à ne pas imprimer (archives…), sans tenir compte de la casse.
    ignore_toggles: Option<Vec<String>>,
}

/// Toggles ignorés par défaut : ceux qui servent d'archive, généralement repliés dans Notion.
const DEFAULT_IGNORED_TOGGLES: [&str; 5] = ["old", "archive", "archives", "ancien", "anciens"];

/// Titres de toggles à ne pas imprimer, en minuscules. Réglable dans le fichier de configuration :
/// `ignore_toggles = ["old", "brouillons"]` (une liste vide imprime tous les toggles).
pub fn ignored_toggles() -> Vec<String> {
    let configured = config_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|raw| toml::from_str::<ConfigFile>(&raw).ok())
        .and_then(|config| config.ignore_toggles);
    match configured {
        Some(titles) => titles.iter().map(|t| t.trim().to_lowercase()).collect(),
        None => DEFAULT_IGNORED_TOGGLES.iter().map(|t| (*t).to_owned()).collect(),
    }
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
