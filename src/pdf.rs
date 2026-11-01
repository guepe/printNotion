use std::sync::OnceLock;

use anyhow::{Result, anyhow};
use typst::foundations::Bytes;
use typst_as_lib::TypstEngine;
use typst_layout::PagedDocument;

/// Compile une source Typst en PDF. `files` fournit les fichiers qu'elle référence (couverture…).
pub fn compile(source: String, files: Vec<(&str, Vec<u8>)>) -> Result<Vec<u8>> {
    let engine = TypstEngine::builder()
        .main_file(source)
        .fonts(fonts().iter().cloned())
        .with_static_file_resolver(files)
        .build();

    let document: PagedDocument = engine
        .compile()
        .output
        .map_err(|e| anyhow!("Échec de la mise en page du document : {e}"))?;

    typst_pdf::pdf(&document, &Default::default())
        .map_err(|e| anyhow!("Échec de la génération du PDF : {e:?}"))
}

/// Polices embarquées, plus les polices système utiles. Lues une seule fois par processus :
/// la police emoji d'Apple pèse près de 200 Mo, l'interface la réutilise d'une génération à l'autre.
fn fonts() -> &'static [Bytes] {
    static FONTS: OnceLock<Vec<Bytes>> = OnceLock::new();
    FONTS.get_or_init(|| {
        typst_assets::fonts()
            .map(Bytes::new)
            .chain(system_fonts().map(Bytes::new))
            .collect()
    })
}

/// Polices système : Avenir Next pour le texte, les emojis d'Apple, et des polices de secours pour
/// les écritures absentes des polices embarquées (japonais, chinois, coréen…). Hors macOS, elles
/// sont absentes et ignorées : le texte retombe sur Libertinus Serif, sans emojis en couleur.
fn system_fonts() -> impl Iterator<Item = Vec<u8>> {
    [
        "/System/Library/Fonts/Avenir Next.ttc",
        "/System/Library/Fonts/Apple Color Emoji.ttc",
        "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
        "/System/Library/Fonts/AppleSDGothicNeo.ttc",
    ]
    .into_iter()
    .filter_map(|path| std::fs::read(path).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Block, Column, Database, Layout, Page, RowGroup, Span};
    use crate::render::page_to_typst;

    fn text(s: &str) -> Vec<Span> {
        vec![Span { text: s.into(), ..Default::default() }]
    }

    #[test]
    fn compiles_a_multi_page_document() {
        let tricky = vec![
            Span { text: "Gras ".into(), bold: true, ..Default::default() },
            Span { text: "italique ".into(), italic: true, ..Default::default() },
            Span { text: "lien".into(), link: Some("https://teconex.be".into()), ..Default::default() },
            Span { text: " #set [x] $y$ \\ \"z\" *a* _b_ @c <d> // e\nligne 2".into(), ..Default::default() },
            Span { text: "cargo build".into(), code: true, ..Default::default() },
        ];
        let mut blocks = vec![
            Block::Heading { level: 1, text: text("Introduction"), children: vec![] },
            Block::Columns(vec![
                Column { width: 0.5, blocks: vec![
                    Block::Heading { level: 3, text: text("Lundi"), children: vec![] },
                    Block::BulletedItem { text: text("Pâtes au pesto"), children: vec![] },
                ] },
                Column { width: 0.5, blocks: vec![
                    Block::Heading { level: 3, text: text("Mardi"), children: vec![] },
                    Block::Toggle { text: text("Toggle"), children: vec![Block::Paragraph { text: text("contenu"), children: vec![] }] },
                ] },
            ]),
            Block::ChildPage { title: "Recettes \"maison\"".into(), icon: Some("🍗".into()), url: "https://www.notion.so/abc".into() },
            Block::Database(Database {
                title: "Menu".into(),
                icon: Some("🍽️".into()),
                view_name: "Cette semaine".into(),
                layout: Layout::Table {
                    headers: vec!["Plat".into(), "Jour".into(), "Fait".into()],
                    widths: vec![280, 200, 40],
                    wrap: false,
                    groups: vec![RowGroup {
                        label: Some(crate::properties::tag("Dinner", "orange")),
                        rows: vec![
                            vec![text("Lasagnes #[x]"), crate::properties::tag("Lundi", "blue"), text("☑")],
                            vec![text("Soupe de potiron et croûtons à l'ail, une recette un peu longue"), vec![], text("☐")],
                        ],
                    }],
                },
            }),
            Block::Database(Database {
                title: "Repas".into(),
                icon: None,
                view_name: "Semaine".into(),
                layout: Layout::Calendar {
                    first_day: chrono::NaiveDate::from_ymd_opt(2026, 10, 26).unwrap(),
                    weeks: 1,
                    month: None,
                    today: chrono::NaiveDate::from_ymd_opt(2026, 10, 30).unwrap(),
                    label: "semaine du 26/10 au 01/11/2026".into(),
                    entries: vec![(chrono::NaiveDate::from_ymd_opt(2026, 10, 29).unwrap(), text("Chili"))],
                },
            }),
            Block::Database(Database {
                title: "Courses".into(),
                icon: None,
                view_name: String::new(),
                layout: Layout::Gallery { cards: vec![vec![text("la coccinelle"), text("note")]] },
            }),
        ];
        for i in 0..40 {
            blocks.push(Block::Paragraph { text: tricky.clone(), children: vec![] });
            blocks.push(Block::BulletedItem {
                text: text(&format!("Puce {i}")),
                children: vec![Block::NumberedItem { text: text("Sous-élément"), children: vec![] }],
            });
        }
        let page = Page { title: "Procédure \"VPN\" #1".into(), icon: Some("🎴".into()), cover: None, blocks };

        let pdf = compile(page_to_typst(&page, "30/10/2026"), Vec::new()).unwrap();
        assert!(pdf.starts_with(b"%PDF"));
        if let Ok(path) = std::env::var("PRINTNOTION_SAMPLE_PDF") {
            std::fs::write(path, &pdf).unwrap();
        }
    }
}
