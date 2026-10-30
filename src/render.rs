//! Traduction du modèle en source Typst.
//!
//! Tout le texte venant de Notion est injecté sous forme de chaînes Typst (`#"…"`),
//! jamais comme balisage brut : aucun caractère saisi dans Notion ne peut donc être
//! interprété comme une commande Typst.

use crate::model::{Block, MAX_PORTRAIT_COLUMNS, Page, Span};

/// Produit le document Typst complet : mise en page A4, en-tête, pied de page et contenu.
/// Le document passe en paysage dès qu'un tableau est trop large pour le portrait.
pub fn page_to_typst(page: &Page, print_date: &str) -> String {
    let flipped = page.blocks.iter().any(|b| {
        b.any(&|b| matches!(b, Block::Table { headers, .. } if headers.len() > MAX_PORTRAIT_COLUMNS))
    });
    let title = string_literal(&page.title);
    let date = string_literal(print_date);
    let mut out = format!(
        r##"#set document(title: {title})
#set page(
  paper: "a4",
  flipped: {flipped},
  margin: (x: 2cm, top: 2.5cm, bottom: 2.5cm),
  header: [
    #set text(size: 9pt, fill: luma(90))
    #{title}
    #v(-0.6em)
    #line(length: 100%, stroke: 0.5pt + luma(180))
  ],
  footer: context [
    #set text(size: 9pt, fill: luma(90))
    #line(length: 100%, stroke: 0.5pt + luma(180))
    #v(-0.6em)
    Imprimé le #{date}
    #h(1fr)
    page #counter(page).display()/#counter(page).final().first()
  ],
)
#set text(lang: "fr", size: 11pt)
#set par(justify: true)
#show link: set text(fill: rgb("#1a5fb4"))
#show link: underline

#align(center, text(size: 20pt, weight: "bold")[#{title}])
#v(1em)

"##
    );
    render_blocks(&page.blocks, &mut out);
    out
}

fn render_blocks(blocks: &[Block], out: &mut String) {
    let mut i = 0;
    while i < blocks.len() {
        match &blocks[i] {
            Block::Heading { level, text, children } => {
                out.push_str(&format!("#heading(level: {level})[{}]\n\n", spans(text)));
                render_blocks(children, out);
                i += 1;
            }
            // Un tableau a besoin de toute la largeur : les colonnes qui en contiennent sont empilées.
            Block::Columns(columns) if columns.iter().flatten().any(|b| b.any(&|b| matches!(b, Block::Table { .. }))) => {
                for column in columns {
                    render_blocks(column, out);
                }
                i += 1;
            }
            Block::Columns(columns) => {
                out.push_str(&format!("#grid(columns: (1fr,) * {}, column-gutter: 1.5em,\n", columns.len().max(1)));
                for column in columns {
                    out.push_str("[\n");
                    render_blocks(column, out);
                    out.push_str("],\n");
                }
                out.push_str(")\n\n");
                i += 1;
            }
            Block::Table { title, headers, rows } => {
                render_table(title, headers, rows, out);
                i += 1;
            }
            Block::ChildPage { title, url } => {
                out.push_str(&format!(
                    "#par[→ #link({})[#{}]]\n\n",
                    string_literal(url),
                    string_literal(title)
                ));
                i += 1;
            }
            Block::Paragraph { text, children } => {
                if text.is_empty() {
                    // Ligne vide dans Notion : simple espacement vertical.
                    out.push_str("#v(0.6em)\n\n");
                } else {
                    out.push_str(&format!("#par[{}]\n\n", spans(text)));
                }
                if !children.is_empty() {
                    out.push_str("#pad(left: 1.5em)[\n");
                    render_blocks(children, out);
                    out.push_str("]\n\n");
                }
                i += 1;
            }
            // Les éléments de liste consécutifs du même type forment une seule liste.
            Block::BulletedItem { .. } | Block::NumberedItem { .. } => {
                let numbered = matches!(blocks[i], Block::NumberedItem { .. });
                let function = if numbered { "enum" } else { "list" };
                out.push_str(&format!("#{function}(\n"));
                while let Some(block) = blocks.get(i) {
                    let (Block::BulletedItem { text, children } | Block::NumberedItem { text, children }) = block
                    else {
                        break;
                    };
                    if matches!(block, Block::NumberedItem { .. }) != numbered {
                        break;
                    }
                    out.push_str(&format!("  [{}", spans(text)));
                    if !children.is_empty() {
                        out.push('\n');
                        render_blocks(children, out);
                    }
                    out.push_str("],\n");
                    i += 1;
                }
                out.push_str(")\n\n");
            }
        }
    }
}

fn render_table(title: &str, headers: &[String], rows: &[Vec<Vec<Span>>], out: &mut String) {
    if !title.is_empty() {
        out.push_str(&format!("#heading(level: 3)[#{}]\n\n", string_literal(title)));
    }
    if rows.is_empty() {
        out.push_str("#par[#emph[(aucune ligne dans cette vue)]]\n\n");
        return;
    }
    out.push_str(&format!(
        "#[\n#set text(size: 9pt)\n#set par(justify: false)\n\
         #table(columns: {}, stroke: 0.5pt + luma(180), inset: 4pt, fill: (_, y) => if y == 0 {{ luma(235) }},\n",
        headers.len()
    ));
    let header_cells: Vec<String> = headers.iter().map(|h| format!("[#strong[#{}]]", string_literal(h))).collect();
    out.push_str(&format!("  table.header({}),\n", header_cells.join(", ")));
    for row in rows {
        let cells: Vec<String> = row.iter().map(|cell| format!("[{}]", spans(cell))).collect();
        out.push_str(&format!("  {},\n", cells.join(", ")));
    }
    out.push_str(")\n]\n\n");
}

fn spans(spans: &[Span]) -> String {
    spans.iter().map(span).collect()
}

fn span(span: &Span) -> String {
    // Les retours à la ligne internes à un bloc deviennent des sauts de ligne Typst.
    let mut content = if span.code {
        format!("#raw({})", string_literal(&span.text))
    } else {
        span.text
            .split('\n')
            .map(|line| format!("#{}", string_literal(line)))
            .collect::<Vec<_>>()
            .join("#linebreak()")
    };
    if span.bold {
        content = format!("#strong[{content}]");
    }
    if span.italic {
        content = format!("#emph[{content}]");
    }
    if span.underline {
        content = format!("#underline[{content}]");
    }
    if span.strikethrough {
        content = format!("#strike[{content}]");
    }
    if let Some(url) = &span.link {
        content = format!("#link({})[{content}]", string_literal(url));
    }
    content
}

/// Littéral de chaîne Typst correctement échappé.
fn string_literal(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => {}
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> Vec<Span> {
        vec![Span { text: s.into(), ..Default::default() }]
    }

    #[test]
    fn escapes_strings() {
        assert_eq!(string_literal(r#"a "b" \c"#), r#""a \"b\" \\c""#);
    }

    #[test]
    fn applies_annotations() {
        let s = Span { text: "x".into(), bold: true, italic: true, link: Some("https://a.b".into()), ..Default::default() };
        assert_eq!(span(&s), r#"#link("https://a.b")[#emph[#strong[#"x"]]]"#);
    }

    #[test]
    fn groups_consecutive_list_items() {
        let blocks = vec![
            Block::NumberedItem { text: text("un"), children: vec![] },
            Block::NumberedItem { text: text("deux"), children: vec![] },
            Block::BulletedItem { text: text("puce"), children: vec![] },
        ];
        let mut out = String::new();
        render_blocks(&blocks, &mut out);
        assert_eq!(out.matches("#enum(").count(), 1);
        assert_eq!(out.matches("#list(").count(), 1);
    }
}
