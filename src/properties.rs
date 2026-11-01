//! Conversion des propriétés d'une ligne de base Notion en contenu de cellule.
//! Les relations, qui demandent des appels à l'API, sont traitées par le client.

use serde_json::Value;

use crate::model::Span;
use crate::notion::rich_text;

/// Contenu imprimable d'une propriété (vide si la propriété n'a pas de valeur).
pub fn cell(prop: &Value) -> Vec<Span> {
    let kind = prop["type"].as_str().unwrap_or_default();
    let value = &prop[kind];
    match kind {
        "title" | "rich_text" => rich_text(value),
        "url" => value
            .as_str()
            .map(|url| vec![Span { text: url.to_owned(), link: Some(url.to_owned()), ..Default::default() }])
            .unwrap_or_default(),
        "checkbox" => text(if value.as_bool() == Some(true) { "☑" } else { "☐" }),
        "formula" => scalar(&value[value["type"].as_str().unwrap_or_default()], value["type"].as_str()),
        "rollup" => match value["type"].as_str() {
            Some("array") => join(value["array"].as_array().map(|items| items.iter().map(cell).collect())),
            // « percent_checked », « percent_per_group »… : une fraction entre 0 et 1.
            Some("number") if value["function"].as_str().is_some_and(|f| f.starts_with("percent")) => {
                value["number"].as_f64().map(|n| text(&format!("{} %", format_number((n * 100.0).round())))).unwrap_or_default()
            }
            other => scalar(&value[other.unwrap_or_default()], other),
        },
        "multi_select" => join(value.as_array().map(|items| items.iter().map(option_tag).collect())),
        "select" | "status" => option_tag(value),
        "people" | "files" => join(value.as_array().map(|items| {
            items.iter().map(|item| text(item["name"].as_str().unwrap_or_default())).collect()
        })),
        "created_by" | "last_edited_by" => text(value["name"].as_str().unwrap_or_default()),
        "unique_id" => match (value["prefix"].as_str(), value["number"].as_i64()) {
            (Some(prefix), Some(n)) => text(&format!("{prefix}-{n}")),
            (None, Some(n)) => text(&n.to_string()),
            _ => Vec::new(),
        },
        _ => scalar(value, Some(kind)),
    }
}

/// Valeurs simples : texte, nombre, date, booléen.
fn scalar(value: &Value, kind: Option<&str>) -> Vec<Span> {
    match (kind, value) {
        (_, Value::Null) => Vec::new(),
        (Some("date"), v) => {
            let start = v["start"].as_str().map(format_date).unwrap_or_default();
            match v["end"].as_str() {
                Some(end) => text(&format!("{start} → {}", format_date(end))),
                None => text(&start),
            }
        }
        (Some("created_time" | "last_edited_time"), Value::String(s)) => text(&format_date(s)),
        (_, Value::String(s)) => text(s),
        (_, Value::Number(n)) => text(&format_number(n.as_f64().unwrap_or_default())),
        (_, Value::Bool(b)) => text(if *b { "☑" } else { "☐" }),
        _ => Vec::new(),
    }
}

/// Option de sélection ou de statut (`{ "name", "color" }`) rendue comme étiquette colorée.
fn option_tag(option: &Value) -> Vec<Span> {
    tag(option["name"].as_str().unwrap_or_default(), option["color"].as_str().unwrap_or("default"))
}

/// Étiquette colorée à la manière de Notion.
pub fn tag(name: &str, color: &str) -> Vec<Span> {
    if name.is_empty() {
        return Vec::new();
    }
    vec![Span { text: name.to_owned(), tag: Some(color.to_owned()), ..Default::default() }]
}

fn text(s: &str) -> Vec<Span> {
    if s.is_empty() { Vec::new() } else { vec![Span { text: s.to_owned(), ..Default::default() }] }
}

/// Assemble plusieurs valeurs : séparées par des virgules, ou par une espace entre étiquettes.
pub fn join(parts: Option<Vec<Vec<Span>>>) -> Vec<Span> {
    let mut out: Vec<Span> = Vec::new();
    for part in parts.unwrap_or_default().into_iter().filter(|p| !p.is_empty()) {
        if let Some(last) = out.last() {
            let between_tags = last.tag.is_some() && part[0].tag.is_some();
            out.extend(text(if between_tags { " " } else { ", " }));
        }
        out.extend(part);
    }
    out
}

/// « 2026-10-30 » → « 30/10/2026 », « 2026-10-30T18:30:00… » → « 30/10/2026 18:30 ».
fn format_date(iso: &str) -> String {
    let Some(date) = iso.get(..10).and_then(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()) else {
        return iso.to_owned();
    };
    let date = date.format("%d/%m/%Y").to_string();
    match iso.get(11..16) {
        Some(time) if iso.len() > 10 => format!("{date} {time}"),
        _ => date,
    }
}

fn format_number(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        // Au plus deux décimales, sans zéros inutiles : 0,9166… → 0,92 ; 2,50 → 2,5.
        format!("{n:.2}").trim_end_matches('0').trim_end_matches('.').replace('.', ",")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn plain(prop: Value) -> String {
        cell(&prop).iter().map(|s| s.text.as_str()).collect()
    }

    #[test]
    fn formats_common_properties() {
        assert_eq!(plain(json!({"type": "select", "select": {"name": "Lundi"}})), "Lundi");
        assert_eq!(plain(json!({"type": "select", "select": null})), "");
        assert_eq!(
            plain(json!({"type": "multi_select", "multi_select": [{"name": "Végé"}, {"name": "Rapide"}]})),
            "Végé Rapide"
        );
        let status = cell(&json!({"type": "status", "status": {"name": "En Stock", "color": "green"}}));
        assert_eq!(status[0].tag.as_deref(), Some("green"));
        assert_eq!(plain(json!({"type": "checkbox", "checkbox": true})), "☑");
        assert_eq!(plain(json!({"type": "number", "number": 4.0})), "4");
        assert_eq!(plain(json!({"type": "number", "number": 2.5})), "2,5");
        assert_eq!(plain(json!({"type": "number", "number": 0.9166666666666666})), "0,92");
        assert_eq!(
            plain(json!({"type": "rollup", "rollup": {"type": "number", "number": 0.9166, "function": "percent_per_group"}})),
            "92 %"
        );
        assert_eq!(plain(json!({"type": "date", "date": {"start": "2026-11-02", "end": null}})), "02/11/2026");
        assert_eq!(
            plain(json!({"type": "date", "date": {"start": "2026-11-02T18:30:00.000+01:00", "end": null}})),
            "02/11/2026 18:30"
        );
        assert_eq!(plain(json!({"type": "formula", "formula": {"type": "string", "string": "ok"}})), "ok");
    }
}
