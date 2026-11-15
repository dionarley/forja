//! Cliente da API do TeamCraft e leitor do vault content-addressed.
//!
//! O TeamCraft guarda os arquivos soltos em um vault por SHA-256
//! (`objects/<2 primeiros>/<2 seguintes>/<sha256>`); a API diz qual arquivo de
//! qual revisão aponta para qual objeto. O serviço nunca copia o arquivo: lê do
//! vault e tessela na hora.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;

/// Um arquivo de uma revisão, como a API do TeamCraft o descreve.
#[derive(Clone, Debug, Deserialize, serde::Serialize)]
pub struct FileRef {
    pub filename: String,
    pub sha256: String,
    pub size_bytes: u64,
    #[serde(default)]
    pub folder: String,
    #[serde(default)]
    pub content_type: String,
    #[serde(default)]
    pub format: String,
    #[serde(default)]
    pub revision_id: String,
}

impl FileRef {
    /// Caminho dentro da revisão (o vault não guarda pastas; a API guarda).
    pub fn path(&self) -> String {
        if self.folder.is_empty() {
            self.filename.clone()
        } else {
            format!("{}/{}", self.folder, self.filename)
        }
    }
}

/// Cliente HTTP do TeamCraft.
pub struct Client {
    base: String,
    token: Option<String>,
    vault: PathBuf,
    agent: ureq::Agent,
}

impl Client {
    /// Lê `FORJA_TEAMCRAFT_URL`, `FORJA_TEAMCRAFT_TOKEN` e `FORJA_VAULT` do ambiente.
    pub fn from_env() -> Result<Self> {
        let base = std::env::var("FORJA_TEAMCRAFT_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:8080".to_string())
            .trim_end_matches('/')
            .to_string();
        let token = std::env::var("FORJA_TEAMCRAFT_TOKEN")
            .ok()
            .filter(|t| !t.trim().is_empty())
            .or_else(|| read_credentials_token());
        let vault = match std::env::var("FORJA_VAULT") {
            Ok(v) => PathBuf::from(v),
            Err(_) => default_vault()?,
        };
        Ok(Self::new(base, token, vault))
    }

    pub fn new(base: impl Into<String>, token: Option<String>, vault: impl Into<PathBuf>) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(5))
            .timeout_read(Duration::from_secs(60))
            .timeout_write(Duration::from_secs(60))
            .build();
        Self {
            base: base.into().trim_end_matches('/').to_string(),
            token: token.filter(|t| !t.trim().is_empty()),
            vault: vault.into(),
            agent,
        }
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    pub fn vault(&self) -> &Path {
        &self.vault
    }

    pub fn has_token(&self) -> bool {
        self.token.is_some()
    }

    fn get(&self, path: &str) -> Result<String> {
        let url = format!("{}{}", self.base, path);
        let mut request = self.agent.get(&url);
        if let Some(token) = &self.token {
            request = request.set("Authorization", &format!("Bearer {token}"));
        }
        let response = request.call().map_err(|e| match e {
            ureq::Error::Status(code, _) => anyhow!("TeamCraft {path}: HTTP {code}"),
            other => anyhow!("TeamCraft {path}: {other}"),
        })?;
        response
            .into_string()
            .with_context(|| format!("TeamCraft {path}: resposta ilegível"))
    }

    fn get_json<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T> {
        let body = self.get(path)?;
        serde_json::from_str(&body).with_context(|| format!("TeamCraft {path}: JSON inesperado"))
    }

    /// Itens do projeto, como a API os devolve (o serviço os repassa inteiros).
    pub fn items(&self, project: Option<&str>) -> Result<serde_json::Value> {
        let query = match project {
            Some(p) if !p.is_empty() => format!("?project={}", encode(p)),
            _ => String::new(),
        };
        self.get_json(&format!("/api/v1/items{query}"))
    }

    /// Detalhe de um item: `{ "item": …, "revisions": [ … ] }`.
    pub fn item(&self, number: &str) -> Result<serde_json::Value> {
        self.get_json(&format!("/api/v1/items/{}", encode(number)))
    }

    /// Arquivos de uma revisão.
    pub fn files(&self, number: &str, revision: &str) -> Result<Vec<FileRef>> {
        self.get_json(&format!(
            "/api/v1/items/{}/revisions/{}/files",
            encode(number),
            encode(revision)
        ))
    }

    /// O objeto do vault de um SHA-256.
    pub fn object(&self, sha256: &str) -> Result<Vec<u8>> {
        if sha256.len() != 64 || !sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(anyhow!("SHA-256 inválido: {sha256}"));
        }
        let object = self
            .vault
            .join("objects")
            .join(&sha256[0..2])
            .join(&sha256[2..4])
            .join(sha256);
        std::fs::read(&object)
            .with_context(|| format!("objeto ausente no vault: {}", object.display()))
    }

    /// Acha o arquivo pelo nome (pasta inclusa, tolerando maiúsculas) e o lê.
    pub fn read_file(&self, number: &str, revision: &str, wanted: &str) -> Result<(FileRef, Vec<u8>)> {
        let files = self.files(number, revision)?;
        let wanted_norm = wanted.trim_start_matches("./").to_ascii_lowercase();
        let found = files
            .iter()
            .find(|f| f.path().to_ascii_lowercase() == wanted_norm)
            .or_else(|| {
                files
                    .iter()
                    .find(|f| f.filename.to_ascii_lowercase() == wanted_norm)
            })
            .cloned()
            .ok_or_else(|| {
                let names = files
                    .iter()
                    .map(FileRef::path)
                    .collect::<Vec<_>>()
                    .join(", ");
                anyhow!("{number}@{revision} não tem '{wanted}' (tem: {names})")
            })?;
        let bytes = self.object(&found.sha256)?;
        Ok((found, bytes))
    }
}

/// `~/.openpdm`-independente: o vault padrão da instalação do TeamCraft do usuário.
fn default_vault() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME não definido")?;
    Ok(PathBuf::from(home).join("teamcraft/data/vault"))
}

/// Sem `FORJA_TEAMCRAFT_TOKEN`, tenta o `credentials.txt` da instalação local.
fn read_credentials_token() -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    let text = std::fs::read_to_string(PathBuf::from(home).join("teamcraft/credentials.txt")).ok()?;
    let mut fallback = None;
    for line in text.lines() {
        let line = line.trim();
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        match key.trim() {
            "admin_token" => return Some(value.to_string()),
            "mcp_token" => fallback = Some(value.to_string()),
            _ => {}
        };
    }
    fallback
}

/// Percent-encoding de um segmento de URL ou valor de query.
pub fn encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Percent-decoding de uma query.
pub fn decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(b) => {
                        out.push(b);
                        i += 3;
                    }
                    Err(_) => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}
