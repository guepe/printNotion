use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};

/// Envoie un fichier à l'imprimante par défaut, ou à l'imprimante nommée, via CUPS (`lp`).
pub fn send_to_printer(file: &Path, printer: Option<&str>) -> Result<()> {
    if cfg!(windows) {
        bail!("L'impression directe n'est pas encore prise en charge sous Windows.");
    }

    let mut command = Command::new("lp");
    if let Some(name) = printer {
        command.args(["-d", name]);
    }
    let output = command
        .arg(file)
        .output()
        .context("Impossible de lancer la commande « lp » (CUPS est-il installé ?)")?;

    if !output.status.success() {
        bail!(
            "L'impression a échoué : {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}
