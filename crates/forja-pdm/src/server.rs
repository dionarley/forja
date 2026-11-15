//! Servidor HTTP do serviço: sem framework assíncrono (o trabalho é CPU: parse
//! de STEP e tesselação), uma thread por requisição e cache por conteúdo.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};
use serde_json::{json, Value};

use crate::kernel::KernelMesh;
use crate::open::{self, Cache, Opened};
use crate::teamcraft::{self, Client};

/// Tudo o que uma requisição precisa.
pub struct App {
    pub client: Client,
    pub cache: Mutex<Cache>,
}

impl App {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            cache: Mutex::new(Cache::default()),
        }
    }
}

/// Sobe o servidor e atende até o processo terminar.
pub fn serve(app: Arc<App>, addr: &str) -> Result<()> {
    let server = tiny_http::Server::http(addr)
        .map_err(|e| anyhow!("não consegui escutar em {addr}: {e}"))?;
    eprintln!("forja-pdm ouvindo em http://{addr}");
    eprintln!("  TeamCraft: {} (token: {})", app.client.base(), if app.client.has_token() { "sim" } else { "não" });
    eprintln!("  vault:     {}", app.client.vault().display());
    for request in server.incoming_requests() {
        let app = Arc::clone(&app);
        std::thread::spawn(move || handle(&app, request));
    }
    Ok(())
}

fn handle(app: &App, request: tiny_http::Request) {
    let (path, query) = split_url(request.url());
    let method = request.method().as_str().to_string();
    if method != "GET" && method != "HEAD" {
        return send(request, 405, "text/plain; charset=utf-8", b"somente GET\n".to_vec());
    }

    let segments: Vec<&str> = path.trim_matches('/').split('/').filter(|s| !s.is_empty()).collect();
    let result: Result<(u16, &'static str, Vec<u8>)> = match segments.as_slice() {
        [] | ["index.html"] => Ok((200, "text/html; charset=utf-8", VIEWER.as_bytes().to_vec())),
        ["healthz"] => Ok((200, JSON, to_json(&health(app)))),
        ["api", "teamcraft", "items"] => {
            let project = query.get("project").map(String::as_str);
            app.client.items(project).map(|v| (200, JSON, to_json(&v)))
        }
        ["api", "teamcraft", "items", number] => app
            .client
            .item(number)
            .map(|v| (200, JSON, to_json(&v))),
        ["api", "teamcraft", "items", number, "revisions", revision, "files"] => app
            .client
            .files(number, revision)
            .and_then(|files| Ok(serde_json::to_value(files)?))
            .map(|v| (200, JSON, to_json(&v))),
        ["api", "open"] => load(app, &query).and_then(|opened| {
            let (metadata, handler) = opened.metadata()?;
            Ok((
                200,
                JSON,
                to_json(&json!({
                    "file": opened.summary(),
                    "handler": handler,
                    "metadata": {
                        "format": metadata.format,
                        "attributes": metadata.attributes,
                    },
                })),
            ))
        }),
        ["api", "mesh"] => mesh(app, &query),
        ["api", "raw"] => load(app, &query).and_then(|opened| {
            let content_type = opened
                .metadata()
                .map(|(m, _)| content_type_for(&m.format))
                .unwrap_or("application/octet-stream");
            Ok((200, content_type, opened.bytes))
        }),
        _ => Err(anyhow!(
            "rota desconhecida: {path} (veja / ou /healthz)"
        )),
    };

    match result {
        Ok((status, ctype, body)) => send(request, status, ctype, body),
        Err(error) => {
            let body = to_json(&json!({ "error": error.to_string() }));
            send(request, 400, JSON, body);
        }
    }
}

fn mesh(app: &App, query: &BTreeMap<String, String>) -> Result<(u16, &'static str, Vec<u8>)> {
    let opened = load(app, query)?;
    let (metadata, handler) = opened.metadata()?;
    let mut cache = app
        .cache
        .lock()
        .map_err(|_| anyhow!("cache envenenado por um erro anterior"))?;
    let (tessellated, cached) = opened.mesh(&mut cache)?;

    if query.get("format").map(String::as_str) == Some("bin") {
        let body = binary_mesh(&tessellated);
        return Ok((200, "application/octet-stream", body));
    }

    Ok((
        200,
        JSON,
        to_json(&json!({
            "file": opened.summary(),
            "handler": handler,
            "cached": cached,
            "metadata": { "format": metadata.format, "attributes": metadata.attributes },
            "kernel": tessellated.report(),
            "mesh": {
                "positions": tessellated.mesh.positions,
                "indices": tessellated.mesh.indices,
            },
        })),
    ))
}

/// `FJPM` + versão + contagens + posições f32 + índices u32 (little-endian).
fn binary_mesh(mesh: &KernelMesh) -> Vec<u8> {
    let mut out = Vec::with_capacity(16 + mesh.mesh.positions.len() * 4 + mesh.mesh.indices.len() * 4);
    out.extend_from_slice(b"FJPM");
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&(mesh.mesh.positions.len() as u32).to_le_bytes());
    out.extend_from_slice(&(mesh.mesh.indices.len() as u32).to_le_bytes());
    for value in &mesh.mesh.positions {
        out.extend_from_slice(&value.to_le_bytes());
    }
    for value in &mesh.mesh.indices {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}

/// Resolve a referência pedida (TeamCraft, SHA do vault ou repositório git).
fn load(app: &App, query: &BTreeMap<String, String>) -> Result<Opened> {
    let source = query
        .get("source")
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_else(|| "teamcraft".to_string());

    match source.as_str() {
        "teamcraft" | "tc" => {
            if let Some(sha) = query.get("sha").filter(|s| !s.is_empty()) {
                return open::from_teamcraft_sha(&app.client, sha);
            }
            let code = required(query, "code")?;
            let revision = match query.get("rev").filter(|r| !r.is_empty()) {
                Some(rev) => rev.clone(),
                None => latest_revision(&app.client, &code)?,
            };
            match query.get("file").filter(|f| !f.is_empty()) {
                Some(file) => open::from_teamcraft(&app.client, &code, &revision, file),
                None => open::open_item(&app.client, &code, &revision),
            }
        }
        "git" => {
            let repo = required(query, "repo")?;
            let path = required(query, "path")?;
            open::from_git(&repo, query.get("rev").map(String::as_str), &path)
        }
        other => Err(anyhow!("fonte desconhecida: {other} (use teamcraft ou git)")),
    }
}

fn latest_revision(client: &Client, code: &str) -> Result<String> {
    let detail = client.item(code)?;
    detail
        .get("revisions")
        .and_then(|r| r.as_array())
        .and_then(|revs| revs.first())
        .and_then(|r| r.get("code"))
        .and_then(|c| c.as_str())
        .map(str::to_string)
        .ok_or_else(|| anyhow!("{code} não tem revisões"))
}

fn required(query: &BTreeMap<String, String>, key: &str) -> Result<String> {
    query
        .get(key)
        .filter(|v| !v.is_empty())
        .cloned()
        .ok_or_else(|| anyhow!("falta o parâmetro '{key}'"))
}

fn health(app: &App) -> Value {
    let cache = app.cache.lock().map(|c| c.len()).unwrap_or(0);
    json!({
        "ok": true,
        "service": "forja-pdm",
        "version": env!("CARGO_PKG_VERSION"),
        "kernel": "solidcore (fork forja)",
        "handlers": "open-pdm-core (fork forja)",
        "cache": cache,
        "teamcraft": open::ping(&app.client),
        "vault": open::vault_status(&app.client),
    })
}

fn content_type_for(format: &str) -> &'static str {
    match format {
        "step" => "model/step",
        "3mf" => "model/3mf",
        "stl" => "model/stl",
        "png" => "image/png",
        "json" => "application/json",
        _ => "application/octet-stream",
    }
}

const JSON: &str = "application/json; charset=utf-8";

fn to_json(value: &Value) -> Vec<u8> {
    serde_json::to_vec(value).unwrap_or_else(|_| b"{\"error\":\"JSON\"}".to_vec())
}

fn send(request: tiny_http::Request, status: u16, content_type: &str, body: Vec<u8>) {
    let mut response = tiny_http::Response::from_data(body).with_status_code(status);
    for (name, value) in [
        ("Content-Type", content_type),
        ("Access-Control-Allow-Origin", "*"),
        ("Cache-Control", "no-store"),
    ] {
        if let Ok(header) = tiny_http::Header::from_bytes(name.as_bytes(), value.as_bytes()) {
            response = response.with_header(header);
        }
    }
    let _ = request.respond(response);
}

/// Separa caminho e query e decodifica a query.
fn split_url(url: &str) -> (String, BTreeMap<String, String>) {
    let (path, query) = match url.split_once('?') {
        Some((path, query)) => (path, query),
        None => (url, ""),
    };
    let mut map = BTreeMap::new();
    for pair in query.split('&').filter(|p| !p.is_empty()) {
        let (key, value) = match pair.split_once('=') {
            Some((k, v)) => (k, v),
            None => (pair, ""),
        };
        map.insert(teamcraft::decode(key), teamcraft::decode(value));
    }
    (path.to_string(), map)
}

/// Visualizador embutido (three.js pelo CDN).
const VIEWER: &str = include_str!("../web/index.html");
