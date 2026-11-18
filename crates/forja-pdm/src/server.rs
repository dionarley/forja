//! Servidor HTTP do serviço: sem framework assíncrono (o trabalho é CPU: parse
//! de STEP e tesselação), uma thread por requisição e cache por conteúdo.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};
use serde_json::{json, Value};

use crate::kernel::KernelMesh;
use crate::native::Design;
use crate::open::{self, Cache, Opened};
use crate::teamcraft::{self, Client};
use crate::{cad, changes};

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
    eprintln!(
        "  TeamCraft: {} (token: {})",
        app.client.base(),
        if app.client.has_token() {
            "sim"
        } else {
            "não"
        }
    );
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
        return send(
            request,
            405,
            "text/plain; charset=utf-8",
            b"somente GET\n".to_vec(),
        );
    }

    let segments: Vec<&str> = path
        .trim_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    let result: Result<(u16, &'static str, Vec<u8>)> = match segments.as_slice() {
        [] | ["index.html"] => Ok((200, "text/html; charset=utf-8", VIEWER.as_bytes().to_vec())),
        // three.js empacotado (MIT): a interface funciona sem internet.
        ["vendor", "three.module.min.js"] => Ok((200, JS, THREE.as_bytes().to_vec())),
        ["vendor", "OrbitControls.js"] => Ok((200, JS, THREE_CONTROLS.as_bytes().to_vec())),
        ["healthz"] => Ok((200, JSON, to_json(&health(app)))),
        // `/api/items` é a rota pública (STYLE §1); `/api/teamcraft/items` fica
        // como apelido para quem já chamava a versão antiga.
        ["api", "items"] | ["api", "teamcraft", "items"] => {
            let project = query.get("project").map(String::as_str);
            app.client.items(project).map(|v| (200, JSON, to_json(&v)))
        }
        ["api", "items", number] | ["api", "teamcraft", "items", number] => {
            app.client.item(number).map(|v| (200, JSON, to_json(&v)))
        }
        ["api", "items", number, "revisions", revision, "files"]
        | ["api", "teamcraft", "items", number, "revisions", revision, "files"] => app
            .client
            .files(number, revision)
            .and_then(|files| Ok(serde_json::to_value(files)?))
            .map(|v| (200, JSON, to_json(&v))),
        ["api", "open"] => load(app, &query).and_then(|opened| {
            let (handler, metadata) = describe(&opened)?;
            Ok((
                200,
                JSON,
                to_json(&json!({
                    "file": opened.summary(),
                    "handler": handler,
                    "metadata": metadata,
                })),
            ))
        }),
        // A montagem nativa: estrutura do `.sc`, com material e massa do PDM.
        ["api", "tree"] => tree(app, &query),
        // A miniatura que o SolidCraft já gravou dentro do `.sc` da peça.
        ["api", "thumb"] => thumbnail(app, &query),
        // A montagem inteira: cada peça nas suas posições, direto do `.sc`.
        ["api", "assembly"] => assembly(app, &query),
        ["api", "mesh"] => mesh(app, &query),
        // Painel de alterações: ECOs do projeto, detalhe com prontidão e diff
        // entre revisões. O fluxo (submeter, revisar, aprovar) é do TeamCraft.
        ["api", "changes"] => {
            let project = query
                .get("project")
                .map(String::as_str)
                .filter(|p| !p.is_empty());
            let limit = query
                .get("limit")
                .and_then(|v| v.parse().ok())
                .unwrap_or(50);
            changes::list(app, project, limit).map(|v| (200, JSON, to_json(&v)))
        }
        ["api", "changes", number] => {
            changes::detail(app, number).map(|v| (200, JSON, to_json(&v)))
        }
        ["api", "diff"] => (|| {
            let number = required(&query, "number")?;
            let from = required(&query, "from")?;
            let to = required(&query, "to")?;
            changes::diff(app, &number, &from, &to).map(|v| (200, JSON, to_json(&v)))
        })(),
        // Leva a peça nativa ao app SolidCraft (e diz onde o arquivo ficou).
        ["api", "cad"] => (|| {
            let code = required(&query, "code")?;
            let revision = match query.get("rev").filter(|r| !r.is_empty()) {
                Some(rev) => rev.clone(),
                None => latest_revision(&app.client, &code)?,
            };
            cad::open(app, &code, &revision).map(|v| (200, JSON, to_json(&v)))
        })(),
        ["api", "raw"] => load(app, &query).map(|opened| {
            let content_type = opened
                .metadata()
                .map(|(m, _)| content_type_for(&m.format))
                .unwrap_or("application/octet-stream");
            (200, content_type, opened.bytes)
        }),
        _ => Err(anyhow!("rota desconhecida: {path} (veja / ou /healthz)")),
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
    if is_native(&opened.name) {
        return Err(anyhow!(
            "{} é o desenho nativo, não malha: peça o STEP da revisão (file=<item>.step)",
            opened.name
        ));
    }
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

/// Metadados de um arquivo aberto; o `.sc` é lido pelo leitor nativo, o resto
/// vai ao registro de handlers do OpenPDM.
fn describe(opened: &Opened) -> Result<(String, Value)> {
    if is_native(&opened.name) {
        let design = Design::open(&opened.bytes)?;
        return Ok((
            "solidcraft-design (nativo)".to_string(),
            json!({ "format": "solidcraft", "attributes": {}, "structure": design.summary() }),
        ));
    }
    let (metadata, handler) = opened.metadata()?;
    Ok((
        handler.to_string(),
        json!({ "format": metadata.format, "attributes": metadata.attributes }),
    ))
}

fn is_native(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".sc")
}

/// `GET /api/tree` — a árvore da montagem a partir do `.sc` nativo.
///
/// Cada definição aparece uma vez, com a quantidade de ocorrências: as quatro
/// cantoneiras são uma peça em quatro posições, não quatro peças.
fn tree(app: &App, query: &BTreeMap<String, String>) -> Result<(u16, &'static str, Vec<u8>)> {
    let opened = load_design(app, query)?;
    let design = Design::open(&opened.bytes)?;
    let mut tree = design.tree();
    if opened.source == "teamcraft" {
        join_pdm(app, &mut tree);
    }
    Ok((
        200,
        JSON,
        to_json(&json!({ "file": opened.summary(), "tree": tree })),
    ))
}

/// `GET /api/thumb?code=PS-001-LM-06` — a miniatura do `.sc` da peça.
fn thumbnail(app: &App, query: &BTreeMap<String, String>) -> Result<(u16, &'static str, Vec<u8>)> {
    let code = required(query, "code")?;
    let revision = match query.get("rev").filter(|r| !r.is_empty()) {
        Some(rev) => rev.clone(),
        None => latest_revision(&app.client, &code)?,
    };
    let files = app.client.files(&code, &revision)?;
    let wanted = query
        .get("file")
        .filter(|f| !f.is_empty())
        .cloned()
        .unwrap_or_else(|| format!("{code}.sc"));
    let found = files
        .iter()
        .find(|f| f.filename.eq_ignore_ascii_case(&wanted))
        .or_else(|| {
            files
                .iter()
                .find(|f| f.filename.to_ascii_lowercase().ends_with(".sc"))
        })
        .ok_or_else(|| anyhow!("{code}@{revision} não tem .sc para miniatura"))?
        .clone();
    let bytes = app.client.object(&found.sha256)?;
    let png = Design::entry(&bytes, "cache/thumbnail.png")?;
    Ok((200, "image/png", png))
}

/// Abre o `.sc` da revisão (o primeiro, se `file` não for dito).
fn load_design(app: &App, query: &BTreeMap<String, String>) -> Result<Opened> {
    if query.get("file").filter(|f| !f.is_empty()).is_some() {
        return load(app, query);
    }
    let source = query
        .get("source")
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_else(|| "teamcraft".to_string());
    if source != "teamcraft" && source != "tc" {
        return Err(anyhow!("fonte {source} precisa do parâmetro 'path'"));
    }
    let code = required(query, "code")?;
    let revision = match query.get("rev").filter(|r| !r.is_empty()) {
        Some(rev) => rev.clone(),
        None => latest_revision(&app.client, &code)?,
    };
    let files = app.client.files(&code, &revision)?;
    let found = files
        .iter()
        .find(|f| is_native(&f.filename))
        .ok_or_else(|| {
            anyhow!(
                "{code}@{revision} não tem .sc (tem: {})",
                files
                    .iter()
                    .map(|f| f.path())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;
    let mut query = query.clone();
    query.insert("file".to_string(), found.filename.clone());
    query.insert("rev".to_string(), revision);
    load(app, &query)
}

/// Junta ao `.sc` o que só o PDM sabe: material, massa e estado da revisão.
fn join_pdm(app: &App, tree: &mut Value) {
    let Some(parts) = tree.get_mut("parts").and_then(Value::as_array_mut) else {
        return;
    };
    for part in parts.iter_mut() {
        let number = part
            .get("part_number")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if number.is_empty() {
            continue;
        }
        let Ok(detail) = app.client.item(&number) else {
            continue;
        };
        let Some(revision) = detail
            .get("revisions")
            .and_then(Value::as_array)
            .and_then(|revisions| revisions.first())
        else {
            continue;
        };
        part["pdm"] = json!({
            "number": number,
            "name": revision.get("name"),
            "revision": revision.get("code"),
            "state": revision.get("state"),
            "material": revision.get("material"),
            "mass_kg": revision.get("mass_kg"),
            "description": revision.get("description"),
        });
    }
}

/// `GET /api/assembly` — a montagem inteira, peça por peça, nas suas posições.
///
/// O STEP exportado do conjunto traz uma cópia por *definição*; aqui cada
/// definição é tessellada uma vez (e fica no cache) e repetida nas ocorrências
/// que o `.sc` registra — as 4 cantoneiras aparecem nas 4 posições.
fn assembly(app: &App, query: &BTreeMap<String, String>) -> Result<(u16, &'static str, Vec<u8>)> {
    let opened = load_design(app, query)?;
    let design = Design::open(&opened.bytes)?;
    let pieces = design.pieces();
    if pieces.is_empty() {
        return Err(anyhow!(
            "{} não é montagem: não tem peça importada",
            opened.name
        ));
    }

    let mut positions: Vec<f32> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut report = Vec::with_capacity(pieces.len());
    {
        let mut cache = app
            .cache
            .lock()
            .map_err(|_| anyhow!("cache envenenado por um erro anterior"))?;
        for piece in &pieces {
            let blob = Design::entry(&opened.bytes, &piece.blob)?;
            let key = format!("{}:{}", opened.key, piece.blob);
            let (mesh, cached) = cache.get(&key, &blob)?;
            for position in &piece.positions {
                let base = (positions.len() / 3) as u32;
                for point in mesh.mesh.positions.chunks(3) {
                    positions.push(point[0] + position[0] as f32);
                    positions.push(point[1] + position[1] as f32);
                    positions.push(point[2] + position[2] as f32);
                }
                for index in &mesh.mesh.indices {
                    indices.push(base + *index);
                }
            }
            report.push(json!({
                "part_number": piece.part_number,
                "name": piece.name,
                "occurrences": piece.positions.len(),
                "triangles": mesh.mesh.indices.len() / 3,
                "cached": cached,
            }));
        }
    }

    if query.get("format").map(String::as_str) == Some("bin") {
        return Ok((
            200,
            "application/octet-stream",
            binary_positions(&positions, &indices),
        ));
    }
    Ok((
        200,
        JSON,
        to_json(&json!({
            "file": opened.summary(),
            "parts": report,
            "mesh": { "positions": positions, "indices": indices },
        })),
    ))
}

/// `FJPM` + versão + contagens + posições f32 + índices u32 (little-endian).
fn binary_mesh(mesh: &KernelMesh) -> Vec<u8> {
    binary_positions(&mesh.mesh.positions, &mesh.mesh.indices)
}

fn binary_positions(positions: &[f32], indices: &[u32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(16 + positions.len() * 4 + indices.len() * 4);
    out.extend_from_slice(b"FJPM");
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&(positions.len() as u32).to_le_bytes());
    out.extend_from_slice(&(indices.len() as u32).to_le_bytes());
    for value in positions {
        out.extend_from_slice(&value.to_le_bytes());
    }
    for value in indices {
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
        other => Err(anyhow!(
            "fonte desconhecida: {other} (use teamcraft ou git)"
        )),
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
const JS: &str = "text/javascript; charset=utf-8";

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

/// Visualizador embutido. O three.js vai junto (`web/vendor/`): o serviço é
/// on-premise e a interface não pode depender de CDN.
const VIEWER: &str = include_str!("../web/index.html");
const THREE: &str = include_str!("../web/vendor/three.module.min.js");
const THREE_CONTROLS: &str = include_str!("../web/vendor/OrbitControls.js");
