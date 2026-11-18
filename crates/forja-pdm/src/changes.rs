//! Painel de alterações (ECO/ECR) e diff entre revisões.
//!
//! O TeamCraft é o dono do fluxo: a Forja só lê, nunca decide. A lista de
//! `/changes` não traz os itens, então o painel junta o detalhe de cada ECO
//! (são poucas) para poder filtrar pelo projeto e dizer o que a alteração leva.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use serde_json::{json, Value};

use crate::server::App;

/// Campos da revisão que o diff compara (código e `seq` ficam de fora: são a
/// identidade da revisão, não um atributo que muda).
const META_FIELDS: [&str; 8] = [
    "name",
    "description",
    "material",
    "classification",
    "unit",
    "state",
    "unit_cost",
    "mass_kg",
];

/// `GET /api/changes` — ECOs do projeto, com os itens de cada uma.
pub fn list(app: &App, project: Option<&str>, limit: usize) -> Result<Value> {
    let raw = app.client.changes(limit)?;
    let entries = raw
        .get("changes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut changes = Vec::with_capacity(entries.len());
    let mut skipped = 0usize;
    for entry in entries {
        let Some(number) = entry.get("number").and_then(Value::as_str) else {
            continue;
        };
        let detail = match app.client.change(number) {
            Ok(detail) => detail,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };
        let items = detail.get("items").cloned().unwrap_or_else(|| json!([]));
        if let Some(project) = project {
            if !belongs(&items, project) {
                continue;
            }
        }
        let mut row = entry;
        row["items"] = items;
        row["review_round"] = detail.get("review_round").cloned().unwrap_or(json!(0));
        row["reviews"] = detail.get("reviews").cloned().unwrap_or_else(|| json!([]));
        changes.push(row);
    }
    Ok(json!({
        "project": project,
        "total": raw.get("total"),
        "skipped": skipped,
        "changes": changes,
    }))
}

/// A ECO leva algum item do projeto? O número do item começa pelo código do
/// projeto (`PS-001-LM-06`), e é o que a lista de itens da ECO dá.
fn belongs(items: &Value, project: &str) -> bool {
    let project = project.to_ascii_uppercase();
    items
        .as_array()
        .map(|items| {
            items.iter().any(|item| {
                item.get("item_number")
                    .and_then(Value::as_str)
                    .map(|number| number.to_ascii_uppercase().starts_with(&project))
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

/// `GET /api/changes/<n>` — detalhe da ECO com a prontidão de liberação.
pub fn detail(app: &App, number: &str) -> Result<Value> {
    let mut value = app.client.change(number)?;
    value["readiness"] = match app.client.readiness(number) {
        Ok(readiness) => readiness,
        Err(error) => json!({ "error": error.to_string() }),
    };
    Ok(value)
}

/// `GET /api/requests` — ECRs do projeto. A lista é rasa; o detalhe de cada uma
/// (itens, ECOs ligadas, discussão) vem de `change_request`.
pub fn requests(app: &App, project: Option<&str>, limit: usize) -> Result<Value> {
    let raw = app.client.change_requests(limit)?;
    let entries = raw
        .get("change_requests")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut requests = Vec::with_capacity(entries.len());
    let mut skipped = 0usize;
    for entry in entries {
        let Some(number) = entry.get("number").and_then(Value::as_str) else {
            continue;
        };
        let detail = match app.client.change_request(number) {
            Ok(detail) => detail,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };
        let items = detail.get("items").cloned().unwrap_or_else(|| json!([]));
        if let Some(project) = project {
            if !belongs(&items, project) {
                continue;
            }
        }
        let mut row = entry;
        row["item_count"] = detail
            .get("items")
            .and_then(Value::as_array)
            .map(|items| json!(items.len()))
            .unwrap_or_else(|| json!(0));
        requests.push(row);
    }
    Ok(json!({
        "project": project,
        "total": raw.get("total"),
        "skipped": skipped,
        "requests": requests,
    }))
}

/// `GET /api/requests/<n>` — a ECR completa. A Forja só mostra: aceitar, adiar,
/// rejeitar e implementar são do TeamCraft (as permissões vêm em `can`).
pub fn request(app: &App, number: &str) -> Result<Value> {
    app.client.change_request(number)
}

/// `GET /api/diff?number=&from=&to=` — o que muda entre duas revisões.
pub fn diff(app: &App, number: &str, from: &str, to: &str) -> Result<Value> {
    let before = app.client.revision(number, from)?;
    let after = app.client.revision(number, to)?;
    Ok(json!({
        "number": number,
        "from": from,
        "to": to,
        "meta": meta_diff(&before, &after),
        "files": files_diff(&before, &after),
        "bom": bom_diff(&before, &after),
    }))
}

/// Um campo da revisão (`null` quando a API não o manda).
fn rev_field(revision: &Value, name: &str) -> Value {
    revision
        .get("revision")
        .and_then(|r| r.get(name))
        .cloned()
        .unwrap_or(Value::Null)
}

fn meta_diff(before: &Value, after: &Value) -> Vec<Value> {
    let mut out = Vec::new();
    for field in META_FIELDS {
        let from = rev_field(before, field);
        let to = rev_field(after, field);
        if from != to {
            out.push(json!({ "field": field, "from": from, "to": to }));
        }
    }
    let from_attrs = rev_field(before, "attributes");
    let to_attrs = rev_field(after, "attributes");
    let keys: BTreeSet<&String> = from_attrs
        .as_object()
        .into_iter()
        .flat_map(|map| map.keys())
        .chain(to_attrs.as_object().into_iter().flat_map(|map| map.keys()))
        .collect();
    for key in keys {
        let from = from_attrs.get(key).cloned().unwrap_or(Value::Null);
        let to = to_attrs.get(key).cloned().unwrap_or(Value::Null);
        if from != to {
            out.push(json!({ "field": key, "from": from, "to": to }));
        }
    }
    out
}

/// Arquivos por caminho dentro da revisão (pasta + nome).
fn file_map(revision: &Value) -> BTreeMap<String, Value> {
    revision
        .get("files")
        .and_then(Value::as_array)
        .map(|files| {
            files
                .iter()
                .filter_map(|file| {
                    let name = file.get("filename").and_then(Value::as_str)?;
                    let folder = file.get("folder").and_then(Value::as_str).unwrap_or("");
                    let path = if folder.is_empty() {
                        name.to_string()
                    } else {
                        format!("{folder}/{name}")
                    };
                    Some((path, file.clone()))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn files_diff(before: &Value, after: &Value) -> Value {
    let from = file_map(before);
    let to = file_map(after);
    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut changed = Vec::new();
    for (path, file) in &to {
        match from.get(path) {
            None => added.push(json!({ "path": path, "bytes": file.get("size_bytes") })),
            Some(old) => {
                let same_sha = old.get("sha256") == file.get("sha256");
                let same_version = old.get("version") == file.get("version");
                if !same_sha || !same_version {
                    changed.push(json!({
                        "path": path,
                        "from_version": old.get("version"),
                        "to_version": file.get("version"),
                        "from_bytes": old.get("size_bytes"),
                        "to_bytes": file.get("size_bytes"),
                    }));
                }
            }
        }
    }
    for path in from.keys() {
        if !to.contains_key(path) {
            removed.push(json!({ "path": path, "bytes": from[path].get("size_bytes") }));
        }
    }
    json!({ "added": added, "removed": removed, "changed": changed })
}

/// Linhas da BOM por número do filho.
fn bom_map(revision: &Value) -> BTreeMap<String, Value> {
    revision
        .get("bom")
        .and_then(Value::as_array)
        .map(|lines| {
            lines
                .iter()
                .filter_map(|line| {
                    let child = line.get("child_number").and_then(Value::as_str)?;
                    Some((child.to_string(), line.clone()))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn bom_diff(before: &Value, after: &Value) -> Value {
    let from = bom_map(before);
    let to = bom_map(after);
    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut changed = Vec::new();
    for (child, line) in &to {
        let brief = |line: &Value| {
            json!({
                "child": child,
                "name": line.get("child_name"),
                "quantity": line.get("quantity"),
            })
        };
        match from.get(child) {
            None => added.push(brief(line)),
            Some(old) => {
                let mut fields = Vec::new();
                for field in ["quantity", "released_child_code", "child_unit", "notes"] {
                    let from = old.get(field).cloned().unwrap_or(Value::Null);
                    let to = line.get(field).cloned().unwrap_or(Value::Null);
                    if from != to {
                        fields.push(json!({ "field": field, "from": from, "to": to }));
                    }
                }
                if !fields.is_empty() {
                    changed.push(json!({
                        "child": child,
                        "name": line.get("child_name"),
                        "fields": fields,
                    }));
                }
            }
        }
    }
    for (child, line) in &from {
        if !to.contains_key(child) {
            removed.push(json!({
                "child": child,
                "name": line.get("child_name"),
                "quantity": line.get("quantity"),
            }));
        }
    }
    json!({ "added": added, "removed": removed, "changed": changed })
}
