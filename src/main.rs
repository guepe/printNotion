mod config;
mod model;
mod notion;
mod pdf;
mod print;
mod properties;
mod render;

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::Parser;

/// Génère un PDF imprimable (A4, en-tête, pied de page) à partir d'une page Notion.
#[derive(Parser)]
#[command(name = "printNotion", version)]
struct Cli {
    /// URL ou ID de la page Notion
    page: String,

    /// Fichier ou dossier de sortie (par défaut : « <titre de la page>.pdf » dans le dossier courant)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Envoie le PDF à l'imprimante par défaut
    #[arg(short, long)]
    print: bool,

    /// Envoie le PDF à l'imprimante nommée (implique --print)
    #[arg(long, value_name = "NOM")]
    printer: Option<String>,
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Erreur : {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    let page_id = notion::parse_page_id(&cli.page)?;
    let token = config::load_token()?;

    let mut client = notion::Client::new(token);
    let page = client.fetch_page(&page_id)?;
    for warning in &client.warnings {
        eprintln!("Avertissement : {warning}");
    }

    let print_date = chrono::Local::now().format("%d/%m/%Y").to_string();
    let pdf = pdf::compile(render::page_to_typst(&page, &print_date))?;

    let path = output_path(cli.output, &page.title);
    std::fs::write(&path, pdf).with_context(|| format!("Impossible d'écrire {}", path.display()))?;
    println!("PDF généré : {}", path.display());

    if cli.print || cli.printer.is_some() {
        print::send_to_printer(&path, cli.printer.as_deref())?;
        println!("Envoyé à l'imprimante {}", cli.printer.as_deref().unwrap_or("par défaut"));
    }
    Ok(())
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
