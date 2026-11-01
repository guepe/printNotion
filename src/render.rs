//! Traduction du modèle en source Typst.
//!
//! Tout le texte venant de Notion est injecté sous forme de chaînes Typst (`#"…"`),
//! jamais comme balisage brut : aucun caractère saisi dans Notion ne peut donc être
//! interprété comme une commande Typst.

use chrono::{Datelike, Duration, NaiveDate};

use crate::model::{Block, Cell, Column, Database, Layout, MAX_PORTRAIT_COLUMNS, Page, RowGroup, Span};

/// Nom sous lequel l'image de couverture est fournie au compilateur Typst.
pub const COVER_FILE: &str = "cover.img";

const MARGIN_X_CM: f64 = 1.8;
const MARGIN_TOP_CM: f64 = 2.2;

/// Produit le document Typst complet : mise en page A4, couverture, en-tête, pied de page et contenu.
/// Le style s'inspire de Notion : police sans empattement, encre gris très foncé, étiquettes
/// colorées, tableaux sans quadrillage vertical.
pub fn page_to_typst(page: &Page, print_date: &str) -> String {
    let flipped = is_landscape(page);
    let title = string_literal(&page.title);
    let date = string_literal(print_date);
    let mut out = format!(
        r##"#set document(title: {title})
#set page(
  paper: "a4",
  flipped: {flipped},
  margin: (x: {MARGIN_X_CM}cm, top: {MARGIN_TOP_CM}cm, bottom: 2cm),
  header: context if counter(page).get().first() > 1 [
    #set text(size: 8pt, fill: luma(140))
    #{title}
    #v(-0.5em)
    #line(length: 100%, stroke: 0.4pt + luma(220))
  ],
  footer: context [
    #set text(size: 8pt, fill: luma(140))
    #line(length: 100%, stroke: 0.4pt + luma(220))
    #v(-0.5em)
    Imprimé le #{date}
    #h(1fr)
    page #counter(page).display()/#counter(page).final().first()
  ],
)
#set text(font: ("Avenir Next", "Apple Color Emoji", "Libertinus Serif"), lang: "fr", size: 9.5pt, fill: rgb("#37352f"))
#set par(justify: false, leading: 0.6em, spacing: 0.85em)
#set list(marker: [•], indent: 0.2em, body-indent: 0.6em)
#set enum(indent: 0.2em, body-indent: 0.6em)
#show heading: set text(fill: rgb("#191919"), weight: "bold")
#show heading.where(level: 1): set text(size: 17pt)
#show heading.where(level: 2): set text(size: 14pt)
#show heading.where(level: 3): set text(size: 12pt)
#show heading: set block(above: 1.5em, below: 0.7em)
#show link: it => underline(stroke: 0.5pt + luma(170), offset: 2pt, it)

"##
    );
    render_title(page, flipped, &mut out);
    render_blocks(&page.blocks, flipped, &mut out);
    out
}

/// Paysage dès qu'un tableau est trop large pour le portrait, ou que la page est un tableau de bord
/// (des bases disposées en colonnes) : c'est ce qui se rapproche le plus d'une page Notion pleine largeur.
fn is_landscape(page: &Page) -> bool {
    let has_database = |b: &Block| b.any(&|b| matches!(b, Block::Database(_)));
    page.blocks.iter().any(|b| {
        b.any(&|b| b.table_width().is_some_and(|w| w > MAX_PORTRAIT_COLUMNS))
            || b.any(&|b| matches!(b, Block::Columns(columns) if columns.iter().any(|c| c.blocks.iter().any(has_database))))
    })
}

/// Couverture pleine largeur, icône qui la chevauche, puis titre — comme l'en-tête d'une page Notion.
fn render_title(page: &Page, flipped: bool, out: &mut String) {
    if page.cover.is_some() {
        let height = if flipped { 4.0 } else { 4.6 };
        let gap = height - MARGIN_TOP_CM - if page.icon.is_some() { 1.1 } else { -0.4 };
        out.push_str(&format!(
            "#place(top + left, dx: -{MARGIN_X_CM}cm, dy: -{MARGIN_TOP_CM}cm, \
             image({}, width: 100% + {}cm, height: {height}cm, fit: \"cover\"))\n#v({gap:.2}cm)\n",
            string_literal(COVER_FILE),
            2.0 * MARGIN_X_CM,
        ));
    }
    if let Some(icon) = &page.icon {
        out.push_str(&format!("#text(size: 40pt)[#{}]\n#v(-0.4em)\n", string_literal(icon)));
    }
    out.push_str(&format!(
        "#text(size: 26pt, weight: \"bold\", fill: rgb(\"#191919\"))[#{}]\n#v(0.4em)\n\n",
        string_literal(&page.title)
    ));
}

fn render_blocks(blocks: &[Block], flipped: bool, out: &mut String) {
    let mut i = 0;
    while i < blocks.len() {
        match &blocks[i] {
            Block::Heading { level, text, children } => {
                out.push_str(&format!("#heading(level: {level})[{}]\n\n", spans(text)));
                render_blocks(children, flipped, out);
                i += 1;
            }
            Block::Columns(columns) => {
                render_columns(columns, flipped, out);
                i += 1;
            }
            Block::Database(database) => {
                render_database(database, out);
                i += 1;
            }
            Block::ChildPage { title, icon, url } => {
                let icon = string_literal(icon.as_deref().unwrap_or("📄"));
                out.push_str(&format!(
                    "#par[#{icon} #link({})[#text(weight: \"medium\")[#{}]]]\n\n",
                    string_literal(url),
                    string_literal(title)
                ));
                i += 1;
            }
            // Imprimé déplié : titre en gras, contenu en retrait souligné d'un filet, comme dans Notion.
            Block::Toggle { text, children } => {
                out.push_str(&format!(
                    "#block(sticky: true, above: 1.3em)[#text(weight: \"semibold\")[▾ {}]]\n\n",
                    spans(text)
                ));
                if !children.is_empty() {
                    out.push_str(
                        "#block(width: 100%, inset: (left: 11pt, y: 2pt), stroke: (left: 1pt + luma(225)))[\n",
                    );
                    render_blocks(children, flipped, out);
                    out.push_str("]\n\n");
                }
                i += 1;
            }
            Block::Paragraph { text, children } => {
                if text.is_empty() {
                    // Ligne vide dans Notion : simple espacement vertical.
                    out.push_str("#v(0.4em)\n\n");
                } else {
                    out.push_str(&format!("#par[{}]\n\n", spans(text)));
                }
                if !children.is_empty() {
                    out.push_str("#pad(left: 1.5em)[\n");
                    render_blocks(children, flipped, out);
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
                        render_blocks(children, flipped, out);
                    }
                    out.push_str("],\n");
                    i += 1;
                }
                out.push_str(")\n\n");
            }
        }
    }
}

/// Colonnes côte à côte avec les proportions de Notion, sauf si une base y serait à l'étroit :
/// elles sont alors empilées pour que chaque base ait toute la largeur.
fn render_columns(columns: &[Column], flipped: bool, out: &mut String) {
    if !columns.iter().all(|c| column_fits(c, flipped)) {
        for column in columns {
            render_blocks(&column.blocks, flipped, out);
        }
        return;
    }
    let widths: Vec<String> = columns.iter().map(|c| format!("{:.1}fr", c.width * 100.0)).collect();
    out.push_str(&format!("#grid(columns: ({},), column-gutter: 1.6em,\n", widths.join(", ")));
    for column in columns {
        out.push_str("[\n");
        render_blocks(&column.blocks, flipped, out);
        out.push_str("],\n");
    }
    out.push_str(")\n\n");
}

/// Une colonne convient si elle offre environ 3 cm par colonne de tableau, et assez de place
/// pour les 7 jours d'un calendrier.
fn column_fits(column: &Column, flipped: bool) -> bool {
    let page_columns = if flipped { 9.0 } else { 6.0 };
    let capacity = ((column.width * page_columns).round() as usize).max(3);
    let too_wide = |b: &Block| match b {
        Block::Database(Database { layout: Layout::Calendar { .. }, .. }) => column.width * page_columns < 5.0,
        b => b.table_width().is_some_and(|w| w > capacity),
    };
    !column.blocks.iter().any(|b| b.any(&too_wide))
}

/// En-tête de base (icône, nom de la base d'origine, onglet de la vue) puis son contenu.
fn render_database(database: &Database, out: &mut String) {
    out.push_str("#block(above: 1.4em, below: 1em, width: 100%)[\n#block(sticky: true, below: 0.6em)[");
    if let Some(icon) = &database.icon {
        out.push_str(&format!("#text(size: 12pt)[#{}] ", string_literal(icon)));
    }
    out.push_str(&format!(
        "#text(size: 12pt, weight: \"bold\", fill: rgb(\"#191919\"))[#{}]",
        string_literal(&database.title)
    ));
    if !database.view_name.is_empty() && database.view_name != database.title {
        out.push_str(&format!(
            " #h(4pt) #box(fill: luma(242), radius: 3pt, inset: (x: 5pt, y: 2pt), outset: (y: 1pt))\
             [#text(size: 7.5pt, fill: luma(100), weight: \"medium\")[#{}]]",
            string_literal(&database.view_name)
        ));
    }
    out.push_str("]\n");
    match &database.layout {
        Layout::Table { headers, widths, groups } => render_groups(headers, widths, groups, out),
        Layout::Calendar { first_day, weeks, month, today, label, entries } => {
            render_calendar(*first_day, *weeks, *month, *today, label, entries, out)
        }
        Layout::Gallery { cards } => render_gallery(cards, out),
    }
    out.push_str("]\n\n");
}

const EMPTY_VIEW: &str = "#text(size: 9pt, fill: luma(150), style: \"italic\")[Aucune ligne dans cette vue]\n";

fn render_groups(headers: &[String], widths: &[u32], groups: &[RowGroup], out: &mut String) {
    if groups.iter().all(|g| g.rows.is_empty()) {
        out.push_str(EMPTY_VIEW);
        return;
    }
    for group in groups {
        if let Some(label) = &group.label {
            out.push_str(&format!(
                "#block(sticky: true, above: 1em, below: 0.4em)[#text(weight: \"semibold\")[▾] {} #h(3pt) \
                 #text(size: 8pt, fill: luma(150))[{}]]\n",
                spans(label),
                group.rows.len()
            ));
        }
        render_table(headers, widths, &group.rows, out);
    }
}

fn render_table(headers: &[String], widths: &[u32], rows: &[Vec<Cell>], out: &mut String) {
    // Les colonnes gardent les proportions qu'elles ont dans Notion, sans descendre sous la largeur
    // du plus long mot de leur en-tête (Notion n'affiche qu'une icône pour une colonne étroite).
    let widths: Vec<String> = widths
        .iter()
        .zip(headers)
        .map(|(width, header)| {
            let longest_word = header.split_whitespace().map(|w| w.chars().count()).max().unwrap_or(0) as u32;
            format!("{}fr", (*width).max(longest_word * 7 + 16))
        })
        .collect();
    out.push_str(&format!(
        "#[\n#set text(size: 8.5pt)\n\
         #table(\n  columns: ({},),\n  inset: (x: 6pt, y: 5pt),\n  align: left + horizon,\n  \
         stroke: (_, y) => (bottom: if y == 0 {{ 0.8pt + luma(170) }} else {{ 0.4pt + luma(228) }}),\n",
        widths.join(", ")
    ));
    let header_cells: Vec<String> = headers
        .iter()
        .enumerate()
        .map(|(i, header)| {
            let longest_cell = rows.iter().filter_map(|row| row.get(i)).map(cell_length).max().unwrap_or(0);
            let lines: Vec<String> =
                wrap_header(header, longest_cell).iter().map(|l| format!("#{}", string_literal(l))).collect();
            format!("[#text(fill: luma(115), weight: \"semibold\")[{}]]", lines.join("#linebreak()"))
        })
        .collect();
    out.push_str(&format!("  table.header({}),\n", header_cells.join(", ")));
    for row in rows {
        let cells: Vec<String> = row.iter().map(|cell| format!("[{}]", spans(cell))).collect();
        out.push_str(&format!("  {},\n", cells.join(", ")));
    }
    out.push_str(")\n]\n");
}

const WEEKDAYS: [&str; 7] = ["lun.", "mar.", "mer.", "jeu.", "ven.", "sam.", "dim."];

/// Grille de calendrier à la Notion : en-tête des jours, numéro en haut à droite (aujourd'hui
/// cerclé de rouge), week-ends grisés, une carte par événement.
fn render_calendar(
    first_day: NaiveDate,
    weeks: usize,
    month: Option<u32>,
    today: NaiveDate,
    label: &str,
    entries: &[(NaiveDate, Cell)],
    out: &mut String,
) {
    let height = if weeks == 1 { "3.2cm" } else { "2.1cm" };
    out.push_str(&format!(
        "#text(size: 8.5pt, weight: \"semibold\", fill: luma(110))[#{}]\n#v(0.2em)\n\
         #grid(columns: (1fr,) * 7, stroke: 0.5pt + luma(228), inset: 0pt,\n",
        string_literal(label)
    ));
    for weekday in WEEKDAYS {
        out.push_str(&format!(
            "  grid.cell(stroke: none, align(center, pad(bottom: 3pt, text(size: 7.5pt, fill: luma(150))[{weekday}]))),\n"
        ));
    }
    for offset in 0..(weeks * 7) as i64 {
        let day = first_day + Duration::days(offset);
        let outside = month.is_some_and(|m| m != day.month());
        let weekend = day.weekday().num_days_from_monday() >= 5;
        let fill = if outside || weekend { "luma(249)" } else { "white" };
        let number = if day == today {
            format!(
                "box(fill: rgb(\"#eb5757\"), radius: 50%, width: 13pt, height: 13pt, \
                 align(center + horizon, text(size: 7.5pt, fill: white, weight: \"bold\")[{}]))",
                day.day()
            )
        } else {
            let color = if outside { "luma(180)" } else { "luma(110)" };
            format!("text(size: 8pt, fill: {color})[{}]", day.day())
        };
        let cards: String = entries
            .iter()
            .filter(|(date, _)| *date == day)
            .map(|(_, cell)| {
                format!(
                    "#block(width: 100%, fill: white, stroke: 0.5pt + luma(215), radius: 3pt, inset: 4pt, below: 3pt)\
                     [#text(size: 7.5pt, weight: \"semibold\")[{}]]",
                    spans(cell)
                )
            })
            .collect();
        out.push_str(&format!(
            "  block(width: 100%, height: {height}, fill: {fill}, inset: 4pt)[#align(right, {number}) #v(1pt) {cards}],\n"
        ));
    }
    out.push_str(")\n");
}

/// Cartes de galerie : autant par ligne que la largeur le permet (environ 5 cm chacune).
fn render_gallery(cards: &[Vec<Cell>], out: &mut String) {
    if cards.is_empty() {
        out.push_str(EMPTY_VIEW);
        return;
    }
    let rendered: Vec<String> = cards
        .iter()
        .map(|card| {
            let mut fields = card.iter();
            let title = fields.next().map(|c| spans(c)).unwrap_or_default();
            let rest: String = fields.map(|c| format!("#v(2pt)#text(size: 8pt)[{}]", spans(c))).collect();
            format!(
                "block(width: 100%, stroke: 0.5pt + luma(220), radius: 4pt, inset: 8pt)\
                 [#text(weight: \"semibold\")[{title}]{rest}]"
            )
        })
        .collect();
    out.push_str(&format!(
        "#layout(size => grid(columns: (1fr,) * calc.max(1, calc.floor(size.width / 5cm)), gutter: 8pt, {}))\n",
        rendered.join(", ")
    ));
}

fn cell_length(cell: &Cell) -> usize {
    cell.iter().map(|s| s.text.chars().count()).sum()
}

/// Coupe un en-tête plus long que le contenu de sa colonne, pour qu'il n'en dicte pas la largeur :
/// « Ingrédients Statut » au-dessus de « 93 % » devient « Ingrédients » / « Statut ».
fn wrap_header(header: &str, longest_cell: usize) -> Vec<String> {
    let longest_word = header.split_whitespace().map(|w| w.chars().count()).max().unwrap_or(0);
    let limit = longest_cell.max(longest_word);
    let mut lines: Vec<String> = Vec::new();
    for word in header.split_whitespace() {
        match lines.last_mut() {
            Some(line) if line.chars().count() + 1 + word.chars().count() <= limit => {
                line.push(' ');
                line.push_str(word);
            }
            _ => lines.push(word.to_owned()),
        }
    }
    if lines.is_empty() { vec![header.to_owned()] } else { lines }
}

fn spans(spans: &[Span]) -> String {
    // Une case à cocher seule est dessinée, plutôt que de dépendre d'un glyphe minuscule.
    if let [only] = spans {
        match only.text.as_str() {
            "☑" => return CHECKED_BOX.to_owned(),
            "☐" => return UNCHECKED_BOX.to_owned(),
            _ => {}
        }
    }
    spans.iter().map(span).collect()
}

const CHECKED_BOX: &str = "#box(width: 9pt, height: 9pt, radius: 2pt, fill: rgb(\"#2383e2\"), baseline: 1.5pt, \
    align(center + horizon, text(size: 7pt, fill: white, weight: \"bold\", font: \"DejaVu Sans Mono\")[✓]))";
const UNCHECKED_BOX: &str = "#box(width: 9pt, height: 9pt, radius: 2pt, stroke: 0.7pt + luma(150), baseline: 1.5pt)";

/// Couleurs des étiquettes Notion (thème clair) : (fond, texte).
fn tag_colors(color: &str) -> (&'static str, &'static str) {
    match color {
        "gray" => ("#e3e2e0", "#32302c"),
        "brown" => ("#eee0da", "#442a1e"),
        "orange" => ("#fadec9", "#49290e"),
        "yellow" => ("#fdecc8", "#402c1b"),
        "green" => ("#dbeddb", "#1c3829"),
        "blue" => ("#d3e5ef", "#183347"),
        "purple" => ("#e8deee", "#412454"),
        "pink" => ("#f5e0e9", "#4c2337"),
        "red" => ("#ffe2dd", "#5d1715"),
        _ => ("#edece9", "#37352f"),
    }
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
    if let Some(color) = &span.tag {
        let (background, ink) = tag_colors(color);
        return format!(
            "#box(fill: rgb(\"{background}\"), radius: 3pt, inset: (x: 4pt, y: 1.5pt), outset: (y: 1pt))\
             [#text(fill: rgb(\"{ink}\"), size: 0.92em)[{content}]]"
        );
    }
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
    fn wraps_headers_longer_than_their_cells() {
        assert_eq!(wrap_header("Ingrédients Statut", 4), ["Ingrédients", "Statut"]);
        assert_eq!(wrap_header("Lieu de stockage", 18), ["Lieu de stockage"]);
        assert_eq!(wrap_header("Planifié", 1), ["Planifié"]);
    }

    #[test]
    fn groups_consecutive_list_items() {
        let blocks = vec![
            Block::NumberedItem { text: text("un"), children: vec![] },
            Block::NumberedItem { text: text("deux"), children: vec![] },
            Block::BulletedItem { text: text("puce"), children: vec![] },
        ];
        let mut out = String::new();
        render_blocks(&blocks, false, &mut out);
        assert_eq!(out.matches("#enum(").count(), 1);
        assert_eq!(out.matches("#list(").count(), 1);
    }
}
