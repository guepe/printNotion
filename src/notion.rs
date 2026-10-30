use std::collections::HashMap;
use std::thread;
use std::time::Duration;

use anyhow::{Result, anyhow, bail};
use serde_json::Value;

use crate::model::{Block, Page, Span};
use crate::properties;

const API_BASE: &str = "https://api.notion.com/v1";
const NOTION_VERSION: &str = "2026-03-11";
const MAX_RETRIES: u32 = 3;

pub struct Client {
    agent: ureq::Agent,
    token: String,
    /// Avertissements collectés pendant la récupération (blocs non gérés…).
    pub warnings: Vec<String>,
    /// Titres des pages liées par des relations, pour ne les demander qu'une fois.
    relation_titles: HashMap<String, String>,
}

impl Client {
    pub fn new(token: String) -> Self {
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .build()
            .into();
        Self { agent, token, warnings: Vec::new(), relation_titles: HashMap::new() }
    }

    /// Récupère le titre et le contenu (récursif) d'une page.
    pub fn fetch_page(&mut self, page_id: &str) -> Result<Page> {
        let page = self.get(&format!("{API_BASE}/pages/{page_id}"))?;
        let title = page_title(&page);
        let blocks = self.fetch_blocks(page_id)?;
        Ok(Page { title, blocks })
    }

    fn fetch_blocks(&mut self, block_id: &str) -> Result<Vec<Block>> {
        let mut blocks = Vec::new();
        for raw in self.fetch_children(block_id)? {
            if let Some(block) = self.convert_block(&raw)? {
                blocks.push(block);
            }
        }
        Ok(blocks)
    }

    fn fetch_children(&self, block_id: &str) -> Result<Vec<Value>> {
        let mut results = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let mut url = format!("{API_BASE}/blocks/{block_id}/children?page_size=100");
            if let Some(c) = &cursor {
                url.push_str(&format!("&start_cursor={c}"));
            }
            let body = self.get(&url)?;
            if let Some(items) = body["results"].as_array() {
                results.extend(items.iter().cloned());
            }
            match body["next_cursor"].as_str() {
                Some(next) if body["has_more"].as_bool() == Some(true) => cursor = Some(next.to_owned()),
                _ => break,
            }
        }
        Ok(results)
    }

    fn convert_block(&mut self, raw: &Value) -> Result<Option<Block>> {
        let kind = raw["type"].as_str().unwrap_or("inconnu");
        let data = &raw[kind];
        let text = rich_text(&data["rich_text"]);
        let has_children = raw["has_children"].as_bool() == Some(true);
        let id = raw["id"].as_str().unwrap_or_default();

        let block = match kind {
            "heading_1" | "heading_2" | "heading_3" | "heading_4" => {
                let level = kind.trim_start_matches("heading_").parse().unwrap_or(1);
                let children = if has_children { self.fetch_blocks(id)? } else { Vec::new() };
                Block::Heading { level, text, children }
            }
            "paragraph" | "toggle" | "bulleted_list_item" | "numbered_list_item" => {
                let children = if has_children { self.fetch_blocks(id)? } else { Vec::new() };
                match kind {
                    "bulleted_list_item" => Block::BulletedItem { text, children },
                    "numbered_list_item" => Block::NumberedItem { text, children },
                    _ => Block::Paragraph { text, children },
                }
            }
            "column_list" => {
                let mut columns = Vec::new();
                for column in self.fetch_children(id)? {
                    let column_id = column["id"].as_str().unwrap_or_default();
                    columns.push(self.fetch_blocks(column_id)?);
                }
                Block::Columns(columns)
            }
            "child_page" => Block::ChildPage {
                title: data["title"].as_str().filter(|t| !t.trim().is_empty()).unwrap_or("Sans titre").to_owned(),
                url: format!("https://www.notion.so/{}", id.replace('-', "")),
            },
            "child_database" => {
                let title = data["title"].as_str().unwrap_or_default();
                match self.fetch_database(id, title) {
                    Ok(table) => table,
                    // Vue liée, base non partagée… : on n'empêche pas l'impression du reste.
                    Err(e) => {
                        self.warn(format!("{e:#}"));
                        return Ok(None);
                    }
                }
            }
            other => {
                self.warn(format!("bloc de type « {other} » non géré, ignoré"));
                return Ok(None);
            }
        };
        Ok(Some(block))
    }

    /// Lit une base intégrée telle que l'affiche sa première vue (filtres, tri et colonnes
    /// visibles compris) et la convertit en tableau.
    fn fetch_database(&mut self, database_id: &str, block_title: &str) -> Result<Block> {
        let display_name = |title: &str| if title.is_empty() { "sans titre".to_owned() } else { title.to_owned() };
        let views = self
            .get(&format!("{API_BASE}/views?database_id={database_id}"))
            .map_err(|e| anyhow!("base « {} » illisible, ignorée : {e}", display_name(block_title)))?;
        let Some(view_id) = views["results"][0]["id"].as_str() else {
            bail!("base « {} » sans vue, ignorée", display_name(block_title));
        };
        let view = self.get(&format!("{API_BASE}/views/{view_id}"))?;

        // Les vues liées s'appellent « Untitled » côté API : le nom de la vue est plus parlant.
        let view_name = view["name"].as_str().unwrap_or_default().trim();
        let title = match block_title.trim() {
            "" | "Untitled" if !view_name.is_empty() => view_name.to_owned(),
            t => t.to_owned(),
        };

        let data_source_id = view["data_source_id"].as_str().unwrap_or_default();
        let schema = self.get(&format!("{API_BASE}/data_sources/{data_source_id}")).map_err(|_| {
            anyhow!(
                "base « {} » ignorée : c'est une vue liée dont la base d'origine n'est pas partagée avec l'intégration \
                 (dans Notion, ouvrez la base d'origine, « ••• » > « Connexions »)",
                display_name(&title)
            )
        })?;
        let columns = view_columns(&view, &schema);

        let mut rows = Vec::new();
        for page_id in self.query_view(view_id)? {
            let page = self.get(&format!("{API_BASE}/pages/{page_id}"))?;
            let props: Vec<&Value> = page["properties"].as_object().map(|p| p.values().collect()).unwrap_or_default();
            let mut row = Vec::new();
            for column in &columns {
                let prop = props.iter().find(|p| p["id"].as_str().map(percent_decode).as_deref() == Some(&column.id));
                row.push(prop.map(|p| self.property_cell(p)).unwrap_or_default());
            }
            rows.push(row);
        }

        // Sans configuration de colonnes dans la vue, on retire celles qui sont vides partout.
        let keep: Vec<bool> = columns
            .iter()
            .enumerate()
            .map(|(i, c)| c.from_view || i == 0 || rows.iter().any(|row: &Vec<Vec<Span>>| !row[i].is_empty()))
            .collect();
        let headers = columns.into_iter().zip(&keep).filter(|(_, k)| **k).map(|(c, _)| c.name).collect();
        let rows = rows
            .into_iter()
            .map(|row| row.into_iter().zip(&keep).filter(|(_, k)| **k).map(|(c, _)| c).collect())
            .collect();
        Ok(Block::Table { title, headers, rows })
    }

    /// IDs des lignes d'une vue, filtrées et triées comme dans Notion.
    fn query_view(&self, view_id: &str) -> Result<Vec<String>> {
        let mut body = self.post(&format!("{API_BASE}/views/{view_id}/queries"), &serde_json::json!({ "page_size": 100 }))?;
        let query_id = body["id"].as_str().unwrap_or_default().to_owned();
        let mut ids = Vec::new();
        loop {
            ids.extend(body["results"].as_array().into_iter().flatten().filter_map(|r| r["id"].as_str().map(str::to_owned)));
            match body["next_cursor"].as_str() {
                Some(next) if body["has_more"].as_bool() == Some(true) => {
                    body = self.get(&format!(
                        "{API_BASE}/views/{view_id}/queries/{query_id}?page_size=100&start_cursor={next}"
                    ))?;
                }
                _ => break,
            }
        }
        Ok(ids)
    }

    fn property_cell(&mut self, prop: &Value) -> Vec<Span> {
        if prop["type"] != "relation" {
            return properties::cell(prop);
        }
        let ids: Vec<String> = prop["relation"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|r| r["id"].as_str().map(str::to_owned))
            .collect();
        let titles = ids.iter().map(|id| self.relation_title(id)).collect();
        properties::join(Some(titles))
    }

    fn relation_title(&mut self, page_id: &str) -> Vec<Span> {
        if !self.relation_titles.contains_key(page_id) {
            let title = match self.get(&format!("{API_BASE}/pages/{page_id}")) {
                Ok(page) => page_title(&page),
                Err(_) => "(page non partagée)".to_owned(),
            };
            self.relation_titles.insert(page_id.to_owned(), title);
        }
        vec![Span { text: self.relation_titles[page_id].clone(), ..Default::default() }]
    }

    fn warn(&mut self, message: String) {
        self.warnings.push(message);
    }

    fn get(&self, url: &str) -> Result<Value> {
        self.send(|| {
            self.agent
                .get(url)
                .header("Authorization", &format!("Bearer {}", self.token))
                .header("Notion-Version", NOTION_VERSION)
                .call()
        })
    }

    fn post(&self, url: &str, body: &Value) -> Result<Value> {
        self.send(|| {
            self.agent
                .post(url)
                .header("Authorization", &format!("Bearer {}", self.token))
                .header("Notion-Version", NOTION_VERSION)
                .send_json(body)
        })
    }

    /// Exécute une requête, en réessayant si l'API demande de ralentir (HTTP 429).
    fn send(&self, call: impl Fn() -> Result<ureq::http::Response<ureq::Body>, ureq::Error>) -> Result<Value> {
        let mut attempt = 0;
        loop {
            let mut response = call().map_err(|e| anyhow!("Impossible de joindre l'API Notion : {e}"))?;

            let status = response.status().as_u16();
            if status == 429 && attempt < MAX_RETRIES {
                let wait = response
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(1);
                thread::sleep(Duration::from_secs(wait));
                attempt += 1;
                continue;
            }

            let body: Value = response
                .body_mut()
                .read_json()
                .map_err(|e| anyhow!("Réponse illisible de l'API Notion (HTTP {status}) : {e}"))?;
            if (200..300).contains(&status) {
                return Ok(body);
            }
            return Err(api_error(status, &body));
        }
    }
}

fn api_error(status: u16, body: &Value) -> anyhow::Error {
    let code = body["code"].as_str().unwrap_or_default();
    let message = body["message"].as_str().unwrap_or_default();
    match (status, code) {
        (401, _) | (_, "unauthorized") => anyhow!(
            "Le token Notion est invalide ou révoqué.\n\
             Vérifiez le « Internal Integration Secret » de votre intégration sur https://www.notion.so/profile/integrations."
        ),
        (404, _) | (_, "object_not_found") | (403, _) | (_, "restricted_resource") => anyhow!(
            "Page introuvable, ou non partagée avec l'intégration.\n\
             Dans Notion, ouvrez la page, menu « ••• » > « Connexions », puis ajoutez votre intégration."
        ),
        _ => anyhow!("Erreur de l'API Notion (HTTP {status}, {code}) : {message}"),
    }
}

struct Column {
    /// Identifiant de propriété, décodé (l'API le renvoie parfois encodé façon URL).
    id: String,
    name: String,
    /// Colonne choisie explicitement dans la configuration de la vue.
    from_view: bool,
}

/// Colonnes à imprimer : celles visibles dans la vue, dans son ordre ; à défaut, toutes
/// les propriétés de la base, titre en premier. Les boutons n'ont rien à imprimer.
fn view_columns(view: &Value, schema: &Value) -> Vec<Column> {
    let schema_props: Vec<(String, String, &str)> = schema["properties"]
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(_, p)| p["type"] != "button")
        .map(|(name, p)| (percent_decode(p["id"].as_str().unwrap_or_default()), name.clone(), p["type"].as_str().unwrap_or_default()))
        .collect();

    let configured: Vec<Column> = view["configuration"]["properties"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|p| p["visible"].as_bool() != Some(false))
        .filter_map(|p| {
            let id = percent_decode(p["property_id"].as_str()?);
            // Une propriété absente du schéma (relation vers une base non partagée…) est ignorée.
            let (_, name, _) = schema_props.iter().find(|(sid, _, _)| *sid == id)?;
            Some(Column { id, name: name.clone(), from_view: true })
        })
        .collect();
    if !configured.is_empty() {
        return configured;
    }

    let mut columns: Vec<&(String, String, &str)> = schema_props.iter().filter(|(_, _, t)| *t == "title").collect();
    columns.extend(schema_props.iter().filter(|(_, _, t)| *t != "title"));
    columns
        .into_iter()
        .map(|(id, name, _)| Column { id: id.clone(), name: name.clone(), from_view: false })
        .collect()
}

/// Décode les séquences « %XX » (ex. « %7DhRT » → « }hRT »).
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if let Some(byte) = s.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_owned())
}

fn page_title(page: &Value) -> String {
    page["properties"]
        .as_object()
        .and_then(|props| props.values().find(|p| p["type"] == "title"))
        .map(|p| plain(&rich_text(&p["title"])))
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| "Sans titre".to_owned())
}

pub fn rich_text(value: &Value) -> Vec<Span> {
    let Some(items) = value.as_array() else { return Vec::new() };
    items
        .iter()
        .map(|item| {
            let a = &item["annotations"];
            let flag = |name: &str| a[name].as_bool() == Some(true);
            Span {
                text: item["plain_text"].as_str().unwrap_or_default().to_owned(),
                bold: flag("bold"),
                italic: flag("italic"),
                underline: flag("underline"),
                strikethrough: flag("strikethrough"),
                code: flag("code"),
                link: item["href"].as_str().map(absolute_link),
            }
        })
        .collect()
}

/// Les liens internes Notion sont relatifs (« /abc123… ») : on les rend absolus.
fn absolute_link(href: &str) -> String {
    if href.starts_with('/') {
        format!("https://www.notion.so{href}")
    } else {
        href.to_owned()
    }
}

fn plain(spans: &[Span]) -> String {
    spans.iter().map(|s| s.text.as_str()).collect()
}

/// Extrait l'ID d'une page depuis une URL Notion ou un ID brut (avec ou sans tirets).
pub fn parse_page_id(input: &str) -> Result<String> {
    let input = input.trim();
    let candidate = match input.split_once("?p=").or_else(|| input.split_once("&p=")) {
        Some((_, rest)) => rest,
        None => input.split(['?', '#']).next().unwrap_or(input),
    };
    let segment = candidate.trim_end_matches('/').rsplit('/').next().unwrap_or(candidate);
    let compact: String = segment.split(['&', '#']).next().unwrap_or(segment).replace('-', "");

    if compact.len() >= 32 {
        let id = &compact[compact.len() - 32..];
        if id.chars().all(|c| c.is_ascii_hexdigit()) {
            let id = id.to_ascii_lowercase();
            return Ok(format!("{}-{}-{}-{}-{}", &id[..8], &id[8..12], &id[12..16], &id[16..20], &id[20..]));
        }
    }
    bail!("« {input} » n'est ni une URL de page Notion ni un ID de page valide")
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "1a2b3c4d-5e6f-7a8b-9c0d-1e2f3a4b5c6d";

    #[test]
    fn parses_raw_ids() {
        assert_eq!(parse_page_id("1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d").unwrap(), ID);
        assert_eq!(parse_page_id(ID).unwrap(), ID);
        assert_eq!(parse_page_id("1A2B3C4D5E6F7A8B9C0D1E2F3A4B5C6D").unwrap(), ID);
    }

    #[test]
    fn parses_urls() {
        for url in [
            "https://www.notion.so/teconex/Ma-page-1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d",
            "https://www.notion.so/Ma-page-1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d?pvs=4",
            "https://www.notion.so/1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d#abc",
            "https://www.notion.so/teconex/0000000000000000000000000000000f?v=1&p=1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d&pm=s",
        ] {
            assert_eq!(parse_page_id(url).unwrap(), ID, "{url}");
        }
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_page_id("https://www.notion.so/ma-page").is_err());
        assert!(parse_page_id("").is_err());
    }

    #[test]
    fn decodes_property_ids() {
        assert_eq!(percent_decode("%7DhRT"), "}hRT");
        assert_eq!(percent_decode("%60%60Xa"), "``Xa");
        assert_eq!(percent_decode("title"), "title");
    }

    #[test]
    fn picks_visible_view_columns_in_view_order() {
        let schema = serde_json::json!({"properties": {
            "Date": {"id": "%7DhRT", "type": "date"},
            "Envoyer": {"id": "pMaf", "type": "button"},
            "Jour": {"id": "title", "type": "title"},
            "Note": {"id": "abc", "type": "rich_text"}
        }});
        let view = serde_json::json!({"configuration": {"properties": [
            {"property_id": "title", "visible": true},
            {"property_id": "}hRT", "visible": true},
            {"property_id": "abc", "visible": false},
            {"property_id": "pMaf", "visible": true},
            {"property_id": "absente", "visible": true}
        ]}});
        let names: Vec<String> = view_columns(&view, &schema).into_iter().map(|c| c.name).collect();
        assert_eq!(names, ["Jour", "Date"]);

        let names: Vec<String> = view_columns(&serde_json::json!({}), &schema).into_iter().map(|c| c.name).collect();
        assert_eq!(names, ["Jour", "Date", "Note"]);
    }

    #[test]
    fn converts_rich_text() {
        let json = serde_json::json!([
            {"plain_text": "gras", "href": null, "annotations": {"bold": true, "italic": false}},
            {"plain_text": "lien", "href": "/abc", "annotations": {"italic": true}}
        ]);
        let spans = rich_text(&json);
        assert!(spans[0].bold && !spans[0].italic);
        assert!(spans[1].italic);
        assert_eq!(spans[1].link.as_deref(), Some("https://www.notion.so/abc"));
    }
}
