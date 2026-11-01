mod config;
mod generate;
mod gui;
mod model;
mod notion;
mod pdf;
mod print;
mod properties;
mod render;

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use clap::Parser;

/// Génère un PDF imprimable (A4, en-tête, pied de page) à partir d'une page Notion.
/// Sans argument, ouvre une petite fenêtre où coller l'URL de la page.
#[derive(Parser)]
#[command(name = "printNotion", version)]
struct Cli {
    /// URL ou ID de la page Notion (absent : ouvre l'interface graphique)
    page: Option<String>,

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
    let cli = Cli::parse();
    let result = match cli.page.clone() {
        Some(page) => run(&page, cli),
        None => gui::run(),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Erreur : {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(page: &str, cli: Cli) -> Result<()> {
    let generated = generate::generate(page, cli.output)?;
    for warning in &generated.warnings {
        eprintln!("Avertissement : {warning}");
    }
    if generated.access_denied > 0 {
        eprintln!(
            "Astuce : partagez les bases d'origine avec l'intégration (« ••• » > « Connexions » dans Notion, \
             ou {}), puis relancez.",
            generate::INTEGRATIONS_URL
        );
    }
    println!("PDF généré : {}", generated.path.display());

    if cli.print || cli.printer.is_some() {
        print::send_to_printer(&generated.path, cli.printer.as_deref())?;
        println!("Envoyé à l'imprimante {}", cli.printer.as_deref().unwrap_or("par défaut"));
    }
    Ok(())
}
