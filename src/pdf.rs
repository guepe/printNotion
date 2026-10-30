use anyhow::{Result, anyhow};
use typst_layout::PagedDocument;
use typst_as_lib::TypstEngine;

/// Compile une source Typst en PDF, avec les polices embarquées dans le binaire.
pub fn compile(source: String) -> Result<Vec<u8>> {
    let engine = TypstEngine::builder()
        .main_file(source)
        .fonts(typst_assets::fonts())
        .build();

    let document: PagedDocument = engine
        .compile()
        .output
        .map_err(|e| anyhow!("Échec de la mise en page du document : {e}"))?;

    typst_pdf::pdf(&document, &Default::default())
        .map_err(|e| anyhow!("Échec de la génération du PDF : {e:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Block, Page, Span};
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
                vec![
                    Block::Heading { level: 3, text: text("Lundi"), children: vec![] },
                    Block::BulletedItem { text: text("Pâtes au pesto"), children: vec![] },
                ],
                vec![
                    Block::Heading { level: 3, text: text("Mardi"), children: vec![] },
                    Block::Paragraph { text: text("Toggle"), children: vec![Block::Paragraph { text: text("contenu"), children: vec![] }] },
                ],
            ]),
            Block::ChildPage { title: "Recettes \"maison\"".into(), url: "https://www.notion.so/abc".into() },
            Block::Table {
                title: "Menu".into(),
                headers: vec!["Plat".into(), "Jour".into(), "Fait".into()],
                rows: vec![
                    vec![text("Lasagnes #[x]"), text("Lundi"), text("☑")],
                    vec![text("Soupe de potiron et croûtons à l'ail, une recette un peu longue"), vec![], text("☐")],
                ],
            },
        ];
        for i in 0..40 {
            blocks.push(Block::Paragraph { text: tricky.clone(), children: vec![] });
            blocks.push(Block::BulletedItem {
                text: text(&format!("Puce {i}")),
                children: vec![Block::NumberedItem { text: text("Sous-élément"), children: vec![] }],
            });
        }
        let page = Page { title: "Procédure \"VPN\" #1".into(), blocks };

        let pdf = compile(page_to_typst(&page, "30/10/2026")).unwrap();
        assert!(pdf.starts_with(b"%PDF"));
        if let Ok(path) = std::env::var("PRINTNOTION_SAMPLE_PDF") {
            std::fs::write(path, &pdf).unwrap();
        }
    }
}
