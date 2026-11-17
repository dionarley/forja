//! `forja-pdm` — o serviço PDM da plataforma Forja.
//!
//! Core do OpenPDM (fork `forja`) para repositório, índice e handlers de
//! formato; kernel do solidcore (fork `forja`) para ler STEP, tessellar e
//! medir. Os arquivos do TeamCraft abrem direto no serviço, sem importação.
//!
//! ```text
//! forja-pdm serve [endereço]              # HTTP em 127.0.0.1:8090
//! forja-pdm tess <arquivo.step|sha256>    # relatório da tesselação
//! forja-pdm open --code PS-001 [--rev B] [--file PS-001.step]
//! forja-pdm health                        # TeamCraft + vault
//! ```

mod handlers;
mod kernel;
mod native;
mod open;
mod server;
mod teamcraft;

use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use serde_json::json;

use crate::server::App;
use crate::teamcraft::Client;

const DEFAULT_ADDR: &str = "127.0.0.1:8090";

fn main() {
    if let Err(error) = run() {
        eprintln!("erro: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args.first().map(String::as_str).unwrap_or("serve");
    let rest: Vec<&str> = args.iter().skip(1).map(String::as_str).collect();

    match command {
        "-h" | "--help" | "help" => {
            print!("{}", help_text());
            Ok(())
        }
        "serve" => {
            let addr = rest
                .first()
                .map(|s| s.to_string())
                .or_else(|| std::env::var("FORJA_PDM_ADDR").ok())
                .unwrap_or_else(|| DEFAULT_ADDR.to_string());
            let client = Client::from_env()?;
            let app = Arc::new(App::new(client));
            server::serve(app, &addr)
        }
        "tess" | "tessellate" => {
            let target = rest
                .first()
                .ok_or_else(|| anyhow!("uso: forja-pdm tess <arquivo.step | sha256>"))?;
            let bytes = read_target(target)?;
            let report = kernel::describe(&bytes)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            Ok(())
        }
        "open" => {
            let flags = flags(&rest)?;
            let client = Client::from_env()?;
            let opened = match (flags.get("code"), flags.get("sha")) {
                (Some(code), _) => {
                    let revision = match flags.get("rev") {
                        Some(rev) => rev.clone(),
                        None => latest_revision(&client, code)?,
                    };
                    match flags.get("file") {
                        Some(file) => open::from_teamcraft(&client, code, &revision, file)?,
                        None => open::open_item(&client, code, &revision)?,
                    }
                }
                (None, Some(sha)) => open::from_teamcraft_sha(&client, sha)?,
                (None, None) => {
                    let repo = flags
                        .get("repo")
                        .ok_or_else(|| anyhow!("uso: forja-pdm open --code PS-001 [--rev B] [--file …]\n     ou: forja-pdm open --repo <dir> --path <arquivo> [--rev <ref>]"))?;
                    let path = flags
                        .get("path")
                        .ok_or_else(|| anyhow!("--repo exige --path"))?;
                    open::from_git(repo, flags.get("rev").map(String::as_str), path)?
                }
            };
            let (metadata, handler) = opened.metadata()?;
            let mut cache = open::Cache::default();
            let (mesh, cached) = opened.mesh(&mut cache)?;
            let report = json!({
                "file": opened.summary(),
                "handler": handler,
                "metadata": { "format": metadata.format, "attributes": metadata.attributes },
                "cached": cached,
                "kernel": mesh.report(),
            });
            println!("{}", serde_json::to_string_pretty(&report)?);
            Ok(())
        }
        "health" => {
            let client = Client::from_env()?;
            let report = json!({
                "teamcraft": open::ping(&client),
                "vault": open::vault_status(&client),
            });
            println!("{}", serde_json::to_string_pretty(&report)?);
            Ok(())
        }
        other => Err(anyhow!("comando desconhecido: {other}\n\n{}", help_text())),
    }
}

fn latest_revision(client: &Client, code: &str) -> Result<String> {
    let detail = client
        .item(code)
        .with_context(|| format!("item {code} no TeamCraft"))?;
    detail
        .get("revisions")
        .and_then(|r| r.as_array())
        .and_then(|revs| revs.first())
        .and_then(|r| r.get("code"))
        .and_then(|c| c.as_str())
        .map(str::to_string)
        .ok_or_else(|| anyhow!("{code} não tem revisões"))
}

/// Lê um caminho local, ou um SHA-256 do vault do TeamCraft.
fn read_target(target: &str) -> Result<Vec<u8>> {
    let path = std::path::Path::new(target);
    if path.is_file() {
        return Ok(std::fs::read(path).with_context(|| format!("ler {target}"))?);
    }
    if target.len() == 64 && target.bytes().all(|b| b.is_ascii_hexdigit()) {
        let client = Client::from_env()?;
        return client.object(target);
    }
    Err(anyhow!("não achei o arquivo {target}"))
}

fn flags(args: &[&str]) -> Result<std::collections::BTreeMap<String, String>> {
    let mut map = std::collections::BTreeMap::new();
    let mut i = 0;
    while i < args.len() {
        let arg = args[i];
        let Some(key) = arg.strip_prefix("--") else {
            return Err(anyhow!("argumento inesperado: {arg}"));
        };
        let (key, value) = match key.split_once('=') {
            Some((k, v)) => (k.to_string(), v.to_string()),
            None => {
                let value = args
                    .get(i + 1)
                    .ok_or_else(|| anyhow!("--{key} exige um valor"))?;
                i += 1;
                (key.to_string(), value.to_string())
            }
        };
        map.insert(key, value);
        i += 1;
    }
    Ok(map)
}

fn help_text() -> String {
    format!(
        "forja-pdm {version} — serviço PDM da Forja

uso:
  forja-pdm serve [endereço]              HTTP (padrão {DEFAULT_ADDR})
  forja-pdm tess <arquivo.step|sha256>    relatório da tesselação pelo kernel
  forja-pdm open --code PS-001 [--rev B] [--file PS-001.step]
  forja-pdm open --repo <dir> --path <arquivo> [--rev <ref>]
  forja-pdm health                        TeamCraft, token e vault

ambiente:
  FORJA_TEAMCRAFT_URL     padrão http://127.0.0.1:8080
  FORJA_TEAMCRAFT_TOKEN   senão usa o admin_token de ~/teamcraft/credentials.txt
  FORJA_VAULT             padrão ~/teamcraft/data/vault
  FORJA_PDM_ADDR          endereço do `serve`
",
        version = env!("CARGO_PKG_VERSION")
    )
}
