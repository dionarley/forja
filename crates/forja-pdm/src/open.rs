//! Abrir um arquivo: do vault do TeamCraft (item + revisão + nome) ou de um
//! repositório git gerenciado pelo core do OpenPDM.
//!
//! É o "abrir direto no serviço": nada é importado, copiado ou convertido antes;
//! a referência é resolvida, os bytes entram no registro de handlers do serviço e
//! o STEP sai tessela do pelo kernel.

use std::path::{Path, PathBuf};
use anyhow::{anyhow, Result};
use open_pdm_core::{FileMetadata, Project};
use serde_json::{json, Value};

use crate::handlers;
use crate::kernel::{self, KernelMesh};
use crate::teamcraft::{Client, FileRef};

/// Um arquivo já resolvido, pronto para metadados e tesselação.
pub struct Opened {
    pub source: String,
    pub reference: String,
    pub name: String,
    pub bytes: Vec<u8>,
    pub sha256: Option<String>,
    pub revision: Option<String>,
    pub item: Option<String>,
    /// Chave de cache: o conteúdo é o mesmo objeto em qualquer revisão.
    pub key: String,
}

impl Opened {
    pub fn metadata(&self) -> Result<(FileMetadata, &'static str)> {
        handlers::describe(&self.name, &self.bytes)
    }

    /// Tessela (ou reaproveita do cache) pelo kernel.
    pub fn mesh(&self, cache: &mut Cache) -> Result<(KernelMesh, bool)> {
        cache.get(&self.key, &self.bytes)
    }

    pub fn summary(&self) -> Value {
        json!({
            "source": self.source,
            "reference": self.reference,
            "name": self.name,
            "bytes": self.bytes.len(),
            "sha256": self.sha256,
            "item": self.item,
            "revision": self.revision,
        })
    }
}

/// Arquivo do TeamCraft: `PS-001`, revisão `B`, `PS-001.step`.
pub fn from_teamcraft(client: &Client, code: &str, revision: &str, file: &str) -> Result<Opened> {
    let (found, bytes) = client.read_file(code, revision, file)?;
    Ok(from_file_ref(code, revision, found, bytes))
}

/// Arquivo do TeamCraft, por SHA-256 (sem precisar da revisão).
pub fn from_teamcraft_sha(client: &Client, sha256: &str) -> Result<Opened> {
    let bytes = client.object(sha256)?;
    Ok(Opened {
        source: "teamcraft".into(),
        reference: format!("sha256:{}", &sha256[..sha256.len().min(12)]),
        name: format!("{}.step", &sha256[..sha256.len().min(12)]),
        bytes,
        sha256: Some(sha256.to_string()),
        revision: None,
        item: None,
        key: sha256.to_string(),
    })
}

fn from_file_ref(code: &str, revision: &str, found: FileRef, bytes: Vec<u8>) -> Opened {
    Opened {
        source: "teamcraft".into(),
        reference: format!("{code}@{revision}/{}", found.path()),
        name: found.filename.clone(),
        sha256: Some(found.sha256.clone()),
        key: found.sha256,
        revision: Some(revision.to_string()),
        item: Some(code.to_string()),
        bytes,
    }
}

/// Arquivo de um repositório git: `rev` vazio é o HEAD.
pub fn from_git(root: &str, revision: Option<&str>, path: &str) -> Result<Opened> {
    let root = PathBuf::from(root);
    let label = root.display().to_string();
    let project = open_project(root)?;
    let revision = revision.filter(|r| !r.is_empty());
    let bytes = project
        .blob(revision, path)
        .map_err(|e| anyhow!("{}: {e}", path))?;
    // O oid do blob é a identidade do conteúdo, como o SHA-256 no TeamCraft.
    let key = project
        .files_at(revision)
        .ok()
        .and_then(|files| files.into_iter().find(|f| f.path == path))
        .map(|f| f.oid)
        .unwrap_or_else(|| format!("{label}:{}:{path}", revision.unwrap_or("HEAD")));
    Ok(Opened {
        source: "git".into(),
        reference: format!(
            "{}@{}/{}",
            project
                .root()
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("repo"),
            revision.unwrap_or("HEAD"),
            path
        ),
        name: Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(path)
            .to_string(),
        bytes,
        sha256: None,
        revision: revision.map(|r| r.to_string()),
        item: None,
        key,
    })
}

fn open_project(root: PathBuf) -> Result<Project> {
    if !root.exists() {
        return Err(anyhow!("repositório não existe: {}", root.display()));
    }
    Project::open(&root).map_err(|e| anyhow!("{}: {e}", root.display()))
}

/// Cache das tesselações recentes, por conteúdo.
#[derive(Default)]
pub struct Cache {
    entries: std::collections::HashMap<String, (std::sync::Arc<KernelMesh>, u128)>,
}

impl Cache {
    pub fn get(&mut self, key: &str, bytes: &[u8]) -> Result<(KernelMesh, bool)> {
        if let Some((mesh, elapsed)) = self.entries.get(key) {
            let mut copy = (**mesh).clone();
            copy.elapsed_ms = *elapsed;
            return Ok((copy, true));
        }
        let mesh = kernel::tessellate(bytes)?;
        let elapsed = mesh.elapsed_ms;
        self.entries.insert(
            key.to_string(),
            (std::sync::Arc::new(mesh.clone()), elapsed),
        );
        Ok((mesh, false))
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

/// Nomes de arquivo prováveis para "abrir o modelo" de um item, quando o
/// chamador não diz qual arquivo quer.
pub fn pick_model(files: &[FileRef]) -> Option<&FileRef> {
    const PREFERRED: [&str; 4] = ["step", "stp", "3mf", "sc"];
    for ext in PREFERRED {
        if let Some(f) = files
            .iter()
            .find(|f| f.format == ext || f.filename.to_ascii_lowercase().ends_with(&format!(".{ext}")))
        {
            return Some(f);
        }
    }
    files.first()
}

/// Abre o modelo mais provável de um item na revisão dada.
pub fn open_item(client: &Client, code: &str, revision: &str) -> Result<Opened> {
    let files = client.files(code, revision)?;
    let found = pick_model(&files)
        .cloned()
        .ok_or_else(|| anyhow!("{code}@{revision} não tem arquivos ({})", files.len()))?;
    let found = found;
    let bytes = client.object(&found.sha256)?;
    Ok(from_file_ref(code, revision, found, bytes))
}

/// O vault existe e é legível?
pub fn vault_status(client: &Client) -> Value {
    let vault = client.vault();
    let objects = vault.join("objects");
    json!({
        "vault": vault.display().to_string(),
        "exists": vault.is_dir(),
        "objects": objects.is_dir(),
        "teamcraft": client.base(),
        "token": client.has_token(),
    })
}

/// Diagnóstico rápido: o projeto do TeamCraft responde?
pub fn ping(client: &Client) -> Value {
    match client.items(None) {
        Ok(items) => {
            let count = items
                .get("items")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            json!({ "ok": true, "items": count })
        }
        Err(error) => json!({ "ok": false, "error": error.to_string() }),
    }
}
