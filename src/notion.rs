use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::{Result, anyhow, bail};
use chrono::{Datelike, NaiveDate};
use serde_json::Value;

use crate::model::{Block, Cell, Column, Database, Layout, Page, RowGroup, Span};
use crate::properties;

const API_BASE: &str = "https://api.notion.com/v1";
const NOTION_VERSION: &str = "2026-03-11";
const MAX_RETRIES: u32 = 3;
/// En dessous de ce nombre de lignes, lire les pages une à une coûte moins qu'une requête groupée.
const BULK_THRESHOLD: usize = 5;
/// Requêtes simultanées maximum : assez pour masquer la latence, assez peu pour ménager
/// la limite de débit de Notion (environ 3 requêtes par seconde en moyenne).
const MAX_CONCURRENT_REQUESTS: usize = 4;

/// Client de l'API Notion. Les blocs frères sont récupérés en parallèle : l'état partagé
/// est donc protégé, et le nombre de requêtes en vol est plafonné.
pub struct Client {
    agent: ureq::Agent,
    token: String,
    /// Avertissements collectés pendant la récupération (blocs non gérés…).
    warnings: Mutex<Vec<String>>,
    /// Nombre de bases ignorées faute d'accès (vues liées vers une base non partagée).
    access_denied: AtomicUsize,
    /// Titres des pages liées par des relations, pour ne les demander qu'une fois.
    relation_titles: Mutex<HashMap<String, String>>,
    /// Nombre de requêtes HTTP en cours, plafonné à `MAX_CONCURRENT_REQUESTS`.
    in_flight: (Mutex<usize>, Condvar),
    /// Titres (en minuscules) des toggles à ne pas imprimer.
    ignored_toggles: Vec<String>,
}

impl Client {
    pub fn new(token: String, ignored_toggles: Vec<String>) -> Self {
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .build()
            .into();
        Self {
            agent,
            token,
            warnings: Mutex::new(Vec::new()),
            access_denied: AtomicUsize::new(0),
            relation_titles: Mutex::new(HashMap::new()),
            in_flight: (Mutex::new(0), Condvar::new()),
            ignored_toggles,
        }
    }

    pub fn warnings(&self) -> Vec<String> {
        self.warnings.lock().unwrap().clone()
    }

    pub fn access_denied(&self) -> usize {
        self.access_denied.load(Ordering::Relaxed)
    }

    /// Récupère le titre et le contenu (récursif) d'une page.
    pub fn fetch_page(&self, page_id: &str) -> Result<Page> {
        let page = self.get(&format!("{API_BASE}/pages/{page_id}"))?;
        let title = page_title(&page);
        let icon = page["icon"]["emoji"].as_str().map(str::to_owned);
        let cover_url = page["cover"]["external"]["url"].as_str().or(page["cover"]["file"]["url"].as_str());
        // Couverture et contenu se téléchargent en parallèle ; une couverture illisible est simplement omise.
        let (cover, blocks) = thread::scope(|scope| {
            let cover = scope.spawn(|| cover_url.and_then(|url| self.download(url).ok()));
            let blocks = self.fetch_blocks(page_id);
            (cover.join().ok().flatten(), blocks)
        });
        Ok(Page { title, icon, cover, blocks: blocks? })
    }

    /// Télécharge un fichier public (couverture…), sans les en-têtes de l'API Notion.
    fn download(&self, url: &str) -> Result<Vec<u8>> {
        // Les couvertures Unsplash de Notion sont en pleine résolution : 1600 px suffisent en A4.
        let url = if url.starts_with("https://images.unsplash.com/") && !url.contains("&w=") {
            format!("{url}&w=1600")
        } else {
            url.to_owned()
        };
        let mut response = self.agent.get(&url).call()?;
        if !response.status().is_success() {
            bail!("HTTP {}", response.status());
        }
        Ok(response.body_mut().with_config().limit(30 * 1024 * 1024).read_to_vec()?)
    }

    fn fetch_blocks(&self, block_id: &str) -> Result<Vec<Block>> {
        let children = self.fetch_children(block_id)?;
        let converted = parallel_map(&children, |raw| self.convert_block(raw));
        converted.into_iter().filter_map(Result::transpose).collect()
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

    fn convert_block(&self, raw: &Value) -> Result<Option<Block>> {
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
            // Toggle d'archive (« old »…) : replié dans Notion, inutile sur papier ; on ne lit même pas son contenu.
            "toggle" if self.ignored_toggles.contains(&plain(&text).trim().to_lowercase()) => return Ok(None),
            "paragraph" | "toggle" | "bulleted_list_item" | "numbered_list_item" => {
                let children = if has_children { self.fetch_blocks(id)? } else { Vec::new() };
                match kind {
                    "bulleted_list_item" => Block::BulletedItem { text, children },
                    "numbered_list_item" => Block::NumberedItem { text, children },
                    "toggle" => Block::Toggle { text, children },
                    _ => Block::Paragraph { text, children },
                }
            }
            "column_list" => {
                let columns = self.fetch_children(id)?;
                let columns = parallel_map(&columns, |column| {
                    let blocks = self.fetch_blocks(column["id"].as_str().unwrap_or_default())?;
                    Ok(Column { width: column["column"]["width_ratio"].as_f64().unwrap_or(0.0), blocks })
                });
                let mut columns = columns.into_iter().collect::<Result<Vec<_>>>()?;
                // Sans ratio fourni par Notion, les colonnes se partagent la largeur à parts égales.
                if columns.iter().any(|c| c.width <= 0.0) {
                    let share = 1.0 / columns.len().max(1) as f64;
                    columns.iter_mut().for_each(|c| c.width = share);
                }
                Block::Columns(columns)
            }
            "child_page" => Block::ChildPage {
                title: data["title"].as_str().filter(|t| !t.trim().is_empty()).unwrap_or("Sans titre").to_owned(),
                // L'icône n'est pas dans le bloc : il faut lire la page (sans gravité si ça échoue).
                icon: self.get(&format!("{API_BASE}/pages/{id}")).ok().and_then(|p| p["icon"]["emoji"].as_str().map(str::to_owned)),
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

    /// Lit une base intégrée telle que l'affiche sa première vue : filtres, tri, colonnes visibles,
    /// groupes, et disposition (tableau, calendrier ou galerie).
    fn fetch_database(&self, database_id: &str, block_title: &str) -> Result<Block> {
        let display_name = |title: &str| if title.is_empty() { "sans titre".to_owned() } else { title.to_owned() };
        let views = self
            .get(&format!("{API_BASE}/views?database_id={database_id}"))
            .map_err(|e| anyhow!("base « {} » illisible, ignorée : {e}", display_name(block_title)))?;
        let Some(view_id) = views["results"][0]["id"].as_str() else {
            bail!("base « {} » sans vue, ignorée", display_name(block_title));
        };
        let view = self.get(&format!("{API_BASE}/views/{view_id}"))?;
        let view_name = view["name"].as_str().unwrap_or_default().trim().to_owned();

        let data_source_id = view["data_source_id"].as_str().unwrap_or_default();
        let schema = match self.get(&format!("{API_BASE}/data_sources/{data_source_id}")) {
            Ok(schema) => schema,
            Err(_) => {
                self.access_denied.fetch_add(1, Ordering::Relaxed);
                let name = if matches!(block_title.trim(), "" | "Untitled") { &view_name } else { block_title };
                bail!(
                    "base « {} » ignorée : c'est une vue liée dont la base d'origine n'est pas partagée avec l'intégration",
                    display_name(name)
                );
            }
        };
        // Comme Notion, on titre la base par le nom et l'icône de sa base d'origine.
        let title = Some(plain(&rich_text(&schema["title"])))
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| block_title.to_owned());
        let icon = schema["icon"]["emoji"].as_str().map(str::to_owned);

        let today = chrono::Local::now().date_naive();
        // Un calendrier n'a souvent aucun filtre : c'est sa plage d'affichage (semaine ou mois)
        // qui restreint les lignes. On la reproduit.
        let calendar = calendar_range(&view, today);
        let columns = view_columns(&view, &schema);

        // La vue donne l'ordre et le filtrage exacts, mais seulement des identifiants : on récupère
        // les lignes par paquets de 100 via la source de données plutôt qu'une par une.
        let ids = self.query_view(view_id)?;
        let mut bulk = if ids.len() > BULK_THRESHOLD {
            self.query_data_source(data_source_id, &view["filter"]).unwrap_or_default()
        } else {
            HashMap::new()
        };
        let mut pages = Vec::new();
        for page_id in ids {
            let page = match bulk.remove(&page_id.replace('-', "")) {
                Some(page) => page,
                None => self.get(&format!("{API_BASE}/pages/{page_id}"))?,
            };
            if let Some(cal) = &calendar {
                if !page_date(&page, &cal.date_property_id).is_some_and(|(start, end)| start <= cal.end && end >= cal.start) {
                    continue;
                }
            }
            pages.push(page);
        }
        let row = |page: &Value| -> Vec<Cell> { columns.iter().map(|c| self.page_cell(page, &c.id)).collect() };

        let layout = match (&calendar, view["configuration"]["type"].as_str()) {
            (Some(cal), _) => {
                let entries = pages
                    .iter()
                    .filter_map(|page| {
                        let (start, _) = page_date(page, &cal.date_property_id)?;
                        Some((start.max(cal.start), self.page_cell(page, "title")))
                    })
                    .collect();
                let first_day = monday_of(cal.start);
                let weeks = ((cal.end - first_day).num_days() as usize) / 7 + 1;
                let month = (cal.end - cal.start).num_days().gt(&7).then(|| cal.start.month());
                Layout::Calendar { first_day, weeks, month, today, label: cal.label.clone(), entries }
            }
            (None, Some("gallery")) => Layout::Gallery {
                cards: pages.iter().map(|page| row(page).into_iter().filter(|cell| !cell.is_empty()).collect()).collect(),
            },
            _ => {
                let rows: Vec<Vec<Cell>> = pages.iter().map(row).collect();
                // Les colonnes que la vue ne nomme pas et qui sont vides partout volent de la place.
                let keep: Vec<bool> = columns
                    .iter()
                    .enumerate()
                    .map(|(i, c)| c.from_view || c.id == "title" || rows.iter().any(|row| !is_blank(&row[i])))
                    .collect();
                let strip = |row: Vec<Cell>| -> Vec<Cell> {
                    row.into_iter().zip(&keep).filter(|(_, k)| **k).map(|(c, _)| c).collect()
                };
                let kept: Vec<&ViewColumn> = columns.iter().zip(&keep).filter(|(_, k)| **k).map(|(c, _)| c).collect();
                let headers = kept.iter().map(|c| c.name.clone()).collect();
                let widths = kept.iter().map(|c| c.width).collect();
                let groups = group_rows(&view, &schema, &pages, rows)
                    .into_iter()
                    .map(|g| RowGroup { label: g.label, rows: g.rows.into_iter().map(strip).collect() })
                    .collect();
                let wrap = view["configuration"]["wrap_cells"].as_bool() == Some(true);
                Layout::Table { headers, widths, wrap, groups }
            }
        };
        Ok(Block::Database(Database { title, icon, view_name, layout }))
    }

    /// Cellule d'une propriété de page ; la cellule titre reprend l'icône de la page, comme dans Notion.
    fn page_cell(&self, page: &Value, property_id: &str) -> Cell {
        let mut cell = page_property(page, property_id).map(|p| self.property_cell(p)).unwrap_or_default();
        if property_id == "title" {
            if let Some(emoji) = page["icon"]["emoji"].as_str() {
                cell.insert(0, Span { text: format!("{emoji} "), ..Default::default() });
            }
        }
        cell
    }

    /// Toutes les lignes d'une source de données répondant au filtre de la vue, indexées par ID sans tirets.
    /// Les filtres rapides n'y sont pas appliqués : le résultat peut contenir plus de lignes que la vue.
    fn query_data_source(&self, data_source_id: &str, filter: &Value) -> Result<HashMap<String, Value>> {
        let mut pages = HashMap::new();
        let mut cursor: Option<String> = None;
        loop {
            let mut query = serde_json::json!({ "page_size": 100 });
            if !filter.is_null() {
                query["filter"] = filter.clone();
            }
            if let Some(c) = &cursor {
                query["start_cursor"] = c.clone().into();
            }
            let body = self.post(&format!("{API_BASE}/data_sources/{data_source_id}/query"), &query)?;
            for page in body["results"].as_array().into_iter().flatten() {
                if let Some(id) = page["id"].as_str() {
                    pages.insert(id.replace('-', ""), page.clone());
                }
            }
            match body["next_cursor"].as_str() {
                Some(next) if body["has_more"].as_bool() == Some(true) => cursor = Some(next.to_owned()),
                _ => break,
            }
        }
        Ok(pages)
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

    fn property_cell(&self, prop: &Value) -> Vec<Span> {
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

    fn relation_title(&self, page_id: &str) -> Vec<Span> {
        let cached = self.relation_titles.lock().unwrap().get(page_id).cloned();
        let title = cached.unwrap_or_else(|| {
            let title = match self.get(&format!("{API_BASE}/pages/{page_id}")) {
                Ok(page) => page_title(&page),
                Err(_) => "(page non partagée)".to_owned(),
            };
            self.relation_titles.lock().unwrap().insert(page_id.to_owned(), title.clone());
            title
        });
        vec![Span { text: title, ..Default::default() }]
    }

    fn warn(&self, message: String) {
        self.warnings.lock().unwrap().push(message);
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
        let _permit = self.acquire_request_slot();
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

/// Libère une place de requête à la fin de sa portée.
struct RequestSlot<'a>(&'a (Mutex<usize>, Condvar));

impl Drop for RequestSlot<'_> {
    fn drop(&mut self) {
        let (count, freed) = self.0;
        *count.lock().unwrap() -= 1;
        freed.notify_one();
    }
}

impl Client {
    fn acquire_request_slot(&self) -> RequestSlot<'_> {
        let (count, freed) = &self.in_flight;
        let mut count = freed.wait_while(count.lock().unwrap(), |n| *n >= MAX_CONCURRENT_REQUESTS).unwrap();
        *count += 1;
        RequestSlot(&self.in_flight)
    }
}

/// Applique `f` à chaque élément dans un thread distinct, en conservant l'ordre des résultats.
fn parallel_map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    if items.len() <= 1 {
        return items.iter().map(f).collect();
    }
    thread::scope(|scope| {
        let f = &f;
        let handles: Vec<_> = items.iter().map(|item| scope.spawn(move || f(item))).collect();
        handles.into_iter().map(|h| h.join().expect("thread de récupération Notion interrompu")).collect()
    })
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

/// Propriété d'une page par identifiant (décodé).
fn page_property<'a>(page: &'a Value, id: &str) -> Option<&'a Value> {
    page["properties"]
        .as_object()?
        .values()
        .find(|p| p["id"].as_str().map(percent_decode).as_deref() == Some(id))
}

/// Début et fin (inclus) de la date d'une page ; une date simple a la même fin que son début.
fn page_date(page: &Value, property_id: &str) -> Option<(NaiveDate, NaiveDate)> {
    let date = &page_property(page, property_id)?["date"];
    let parse = |v: &Value| v.as_str()?.get(..10).and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok());
    let start = parse(&date["start"])?;
    Some((start, parse(&date["end"]).unwrap_or(start)))
}

/// Plage affichée par une vue calendrier : la semaine (lundi → dimanche) ou le mois en cours.
struct CalendarRange {
    date_property_id: String,
    start: NaiveDate,
    end: NaiveDate,
    label: String,
}

fn calendar_range(view: &Value, today: NaiveDate) -> Option<CalendarRange> {
    let config = &view["configuration"];
    if config["type"] != "calendar" {
        return None;
    }
    let date_property_id = percent_decode(config["date_property_id"].as_str()?);
    let (start, end, label) = if config["view_range"] == "week" {
        let start = today - chrono::Duration::days(i64::from(today.weekday().num_days_from_monday()));
        let end = start + chrono::Duration::days(6);
        let label = format!("semaine du {} au {}", start.format("%d/%m"), end.format("%d/%m/%Y"));
        (start, end, label)
    } else {
        let start = today.with_day(1)?;
        let next_month = if start.month() == 12 {
            NaiveDate::from_ymd_opt(start.year() + 1, 1, 1)?
        } else {
            NaiveDate::from_ymd_opt(start.year(), start.month() + 1, 1)?
        };
        let label = format!("{} {}", MONTHS[start.month0() as usize], start.year());
        (start, next_month.pred_opt()?, label)
    };
    Some(CalendarRange { date_property_id, start, end, label })
}

const MONTHS: [&str; 12] = [
    "janvier", "février", "mars", "avril", "mai", "juin",
    "juillet", "août", "septembre", "octobre", "novembre", "décembre",
];

/// Cellule sans rien d'imprimable (Notion renvoie parfois des morceaux de texte vides).
fn is_blank(cell: &Cell) -> bool {
    cell.iter().all(|span| span.text.trim().is_empty())
}

fn monday_of(day: NaiveDate) -> NaiveDate {
    day - chrono::Duration::days(i64::from(day.weekday().num_days_from_monday()))
}

/// Répartit les lignes selon la propriété de regroupement de la vue (sélection, multi-sélection
/// ou statut), dans l'ordre des options de la base. Les groupes vides ne sont pas imprimés.
fn group_rows(view: &Value, schema: &Value, pages: &[Value], rows: Vec<Vec<Cell>>) -> Vec<RowGroup> {
    let group_by = &view["configuration"]["group_by"];
    let kind = group_by["type"].as_str().unwrap_or_default();
    let property_id = group_by["property_id"].as_str().map(percent_decode);
    let options = property_id.as_deref().and_then(|id| {
        let prop = schema["properties"].as_object()?.values().find(|p| p["id"].as_str().map(percent_decode).as_deref() == Some(id))?;
        prop[kind]["options"].as_array().cloned()
    });
    let (Some(property_id), Some(options), "select" | "multi_select" | "status") = (property_id, options, kind) else {
        return vec![RowGroup { label: None, rows }];
    };

    let values = |page: &Value| -> Vec<String> {
        let value = page_property(page, &property_id).map(|p| &p[kind]);
        match value {
            Some(Value::Array(items)) => items.iter().filter_map(|i| i["name"].as_str().map(str::to_owned)).collect(),
            Some(v) => v["name"].as_str().map(|n| vec![n.to_owned()]).unwrap_or_default(),
            None => Vec::new(),
        }
    };
    let page_values: Vec<Vec<String>> = pages.iter().map(values).collect();

    // Sur papier, une ligne n'est imprimée qu'une fois : dans le premier de ses groupes.
    let mut placed = vec![false; rows.len()];
    let mut groups: Vec<RowGroup> = options
        .iter()
        .map(|option| {
            let name = option["name"].as_str().unwrap_or_default();
            let mut group_rows = Vec::new();
            for (i, values) in page_values.iter().enumerate() {
                if !placed[i] && values.iter().any(|n| n == name) {
                    placed[i] = true;
                    group_rows.push(rows[i].clone());
                }
            }
            RowGroup { label: Some(properties::tag(name, option["color"].as_str().unwrap_or("default"))), rows: group_rows }
        })
        .collect();
    let property_name = group_by["property_name"].as_str().unwrap_or("cette propriété");
    let without: Vec<Vec<Cell>> = rows.iter().zip(&page_values).filter(|(_, v)| v.is_empty()).map(|(r, _)| r.clone()).collect();
    groups.push(RowGroup {
        label: Some(vec![Span { text: format!("Sans « {property_name} »"), ..Default::default() }]),
        rows: without,
    });
    groups.retain(|g| !g.rows.is_empty());
    groups
}

struct ViewColumn {
    /// Identifiant de propriété, décodé (l'API le renvoie parfois encodé façon URL).
    id: String,
    name: String,
    /// Largeur de la colonne dans Notion, en pixels.
    width: u32,
    /// Colonne choisie explicitement dans la configuration de la vue.
    from_view: bool,
}

/// Colonnes à imprimer : celles visibles dans la vue, dans son ordre ; à défaut, toutes
/// les propriétés de la base, titre en premier. Les boutons n'ont rien à imprimer.
fn view_columns(view: &Value, schema: &Value) -> Vec<ViewColumn> {
    let is_table = matches!(view["configuration"]["type"].as_str(), None | Some("table"));
    let schema_props: Vec<(String, String, &str)> = schema["properties"]
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(_, p)| p["type"] != "button")
        .map(|(name, p)| (percent_decode(p["id"].as_str().unwrap_or_default()), name.clone(), p["type"].as_str().unwrap_or_default()))
        .collect();

    let configured: Vec<ViewColumn> = view["configuration"]["properties"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|p| p["visible"].as_bool() != Some(false))
        .filter_map(|p| {
            let id = percent_decode(p["property_id"].as_str()?);
            // Une propriété absente du schéma (relation vers une base non partagée…) est ignorée.
            let (_, name, _) = schema_props.iter().find(|(sid, _, _)| *sid == id)?;
            let width = p["width"].as_u64().map_or(DEFAULT_COLUMN_WIDTH, |w| w as u32);
            Some(ViewColumn { id, name: name.clone(), width, from_view: true })
        })
        .collect();
    if !configured.is_empty() {
        let mut columns = configured;
        // Dans un tableau qui ne masque rien explicitement, les propriétés que la vue ne mentionne
        // pas restent visibles, à la suite. Si la vue masque des colonnes, Notion masque aussi celles
        // ajoutées depuis. Dans une galerie ou un calendrier, seules les propriétés cochées s'affichent.
        let hides_some = view["configuration"]["properties"]
            .as_array()
            .is_some_and(|props| props.iter().any(|p| p["visible"].as_bool() == Some(false)));
        if is_table && !hides_some {
            let listed: Vec<String> = view["configuration"]["properties"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|p| p["property_id"].as_str().map(percent_decode))
                .collect();
            columns.extend(
                schema_props
                    .iter()
                    .filter(|(id, _, _)| !listed.contains(id))
                    .map(|(id, name, _)| ViewColumn { id: id.clone(), name: name.clone(), width: DEFAULT_COLUMN_WIDTH, from_view: false }),
            );
        }
        if !columns.iter().any(|c| c.id == "title") {
            columns.insert(0, ViewColumn { id: "title".into(), name: String::new(), width: TITLE_COLUMN_WIDTH, from_view: true });
        }
        return columns;
    }

    let mut columns: Vec<&(String, String, &str)> = schema_props.iter().filter(|(_, _, t)| *t == "title").collect();
    columns.extend(schema_props.iter().filter(|(_, _, t)| *t != "title"));
    columns
        .into_iter()
        .map(|(id, name, t)| ViewColumn {
            id: id.clone(),
            name: name.clone(),
            width: if *t == "title" { TITLE_COLUMN_WIDTH } else { DEFAULT_COLUMN_WIDTH },
            from_view: false,
        })
        .collect()
}

/// Largeurs par défaut des colonnes dans Notion, en pixels.
const DEFAULT_COLUMN_WIDTH: u32 = 200;
const TITLE_COLUMN_WIDTH: u32 = 280;

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
                tag: None,
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
    fn computes_calendar_ranges() {
        let today = NaiveDate::from_ymd_opt(2026, 10, 31).unwrap(); // un samedi
        let week = serde_json::json!({"configuration": {"type": "calendar", "date_property_id": "CDqJ", "view_range": "week"}});
        let range = calendar_range(&week, today).unwrap();
        assert_eq!((range.start.to_string(), range.end.to_string()), ("2026-10-26".into(), "2026-11-01".into()));
        assert_eq!(range.label, "semaine du 26/10 au 01/11/2026");

        let month = serde_json::json!({"configuration": {"type": "calendar", "date_property_id": "%7DhRT"}});
        let range = calendar_range(&month, NaiveDate::from_ymd_opt(2026, 12, 15).unwrap()).unwrap();
        assert_eq!((range.start.to_string(), range.end.to_string()), ("2026-12-01".into(), "2026-12-31".into()));
        assert_eq!((range.date_property_id.as_str(), range.label.as_str()), ("}hRT", "décembre 2026"));

        assert!(calendar_range(&serde_json::json!({"configuration": {"type": "table"}}), today).is_none());
    }

    #[test]
    fn reads_page_dates() {
        let page = serde_json::json!({"properties": {"Date": {"id": "CDqJ", "type": "date",
            "date": {"start": "2026-10-28T19:00:00.000+01:00", "end": null}}}});
        let day = NaiveDate::from_ymd_opt(2026, 10, 28).unwrap();
        assert_eq!(page_date(&page, "CDqJ"), Some((day, day)));
        assert_eq!(page_date(&page, "autre"), None);
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

        // Une vue qui ne masque rien laisse voir les propriétés qu'elle ne mentionne pas.
        let partial = serde_json::json!({"configuration": {"type": "table", "properties": [
            {"property_id": "title", "visible": true, "width": 120}
        ]}});
        let columns = view_columns(&partial, &schema);
        let names: Vec<&str> = columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["Jour", "Date", "Note"]);
        assert_eq!(columns[0].width, 120);

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
