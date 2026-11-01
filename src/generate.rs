use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::{config, notion, pdf, render};

/// Résultat d'une génération : le PDF écrit et les avertissements rencontrés.
pub struct Generated {
    pub path: PathBuf,
    pub warnings: Vec<String>,
    /// Bases ignorées faute d'accès : l'utilisateur doit les partager avec l'intégration.
    pub access_denied: usize,
}

/// Page Notion où l'on gère les intégrations et les pages auxquelles elles ont accès.
pub const INTEGRATIONS_URL: &str = "https://www.notion.so/profile/integrations";

/// Récupère une page Notion et l'écrit en PDF.
/// `output` peut être un fichier, un dossier, ou absent (dossier courant).
pub fn generate(page: &str, output: Option<PathBuf>) -> Result<Generated> {
    let page_id = notion::parse_page_id(page)?;
    let token = config::load_token()?;

    let client = notion::Client::new(token, config::ignored_toggles());
    let page = client.fetch_page(&page_id)?;

    let print_date = chrono::Local::now().format("%d/%m/%Y").to_string();
    let files = page.cover.clone().map(|cover| (render::COVER_FILE, cover)).into_iter().collect();
    let pdf = pdf::compile(render::page_to_typst(&page, &print_date), files)?;

    let path = output_path(output, &page.title);
    std::fs::write(&path, pdf).with_context(|| format!("Impossible d'écrire {}", path.display()))?;
    Ok(Generated { path, warnings: client.warnings(), access_denied: client.access_denied() })
}

fn output_path(output: Option<PathBuf>, title: &str) -> PathBuf {
    let file_name = format!("{}.pdf", sanitize_file_name(title));
    match output {
        Some(path) if path.is_dir() => path.join(file_name),
        Some(path) => path,
        None => PathBuf::from(file_name),
    }
}

/// Retire les caractères interdits dans un nom de fichier (macOS, Linux, Windows).
fn sanitize_file_name(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| if c.is_control() || r#"/\:*?"<>|"#.contains(c) { '_' } else { c })
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').trim();
    if cleaned.is_empty() { "Sans titre".to_owned() } else { cleaned.to_owned() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_file_names() {
        assert_eq!(sanitize_file_name("Procédure : VPN / Wi-Fi"), "Procédure _ VPN _ Wi-Fi");
        assert_eq!(sanitize_file_name("  ...  "), "Sans titre");
    }
}
