//! Leitura do `.sc` nativo do SolidCraft: a montagem e a peça.
//!
//! O `.sc` é um zip com `document.json` (componentes, links, ocorrências e a
//! linha do tempo), `cache/model.json`, `cache/bodies.bin` e, quando há peça
//! importada, `blobs/<sha>.step`. Este módulo lê o que o PDM precisa mostrar —
//! a estrutura e a miniatura nativa — sem converter nada: quem quiser a
//! geometria abre o arquivo no SolidCraft, e quem quiser a malha pede o STEP
//! derivado ao kernel.
//!
//! O formato não é documentado; aqui só se usa o que é estável entre versões:
//! `components` (id → nome, metadados de peça), `features` do tipo `import`
//! (componente, arquivo, corpos) e `occurrences` (transformação).

use std::io::Read;

use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};

/// Um `.sc` aberto: o JSON do documento, o que dá para ler dele.
pub struct Design {
    pub document: Value,
}

impl Design {
    pub fn open(bytes: &[u8]) -> Result<Self> {
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
            .context("não é um .sc (zip ilegível)")?;
        let document = {
            let mut file = archive
                .by_name("document.json")
                .map_err(|_| anyhow!("zip sem document.json: não é um .sc do SolidCraft"))?;
            let mut text = String::new();
            file.read_to_string(&mut text)
                .context("document.json ilegível")?;
            serde_json::from_str(&text).context("document.json não é JSON")?
        };
        Ok(Self { document })
    }

    /// Um arquivo qualquer do zip (`cache/thumbnail.png`, por exemplo).
    pub fn entry(bytes: &[u8], name: &str) -> Result<Vec<u8>> {
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
            .context("não é um .sc (zip ilegível)")?;
        let mut file = archive
            .by_name(name)
            .map_err(|_| anyhow!("o .sc não tem {name}"))?;
        let mut out = Vec::with_capacity(file.size() as usize);
        file.read_to_end(&mut out)?;
        Ok(out)
    }

    pub fn name(&self) -> &str {
        self.document
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
    }

    /// A árvore da montagem: uma entrada por definição, com as ocorrências.
    pub fn tree(&self) -> Value {
        let doc = &self.document;
        let components = doc
            .get("components")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let occurrences = doc
            .get("occurrences")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let features = doc
            .get("features")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let links = doc
            .get("links")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();

        let mut parts = Vec::new();
        for component in &components {
            let id = component.get("id").and_then(Value::as_u64).unwrap_or(0);
            let metadata = component.get("metadata").cloned().unwrap_or(Value::Null);
            let string = |key: &str| {
                metadata
                    .get(key)
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string()
            };
            // A importação daquele componente é a definição da peça.
            let import = features.iter().find(|f| {
                f.get("type").and_then(Value::as_str) == Some("import")
                    && f.get("component").and_then(Value::as_u64) == Some(id)
            });
            let bodies = import
                .and_then(|f| f.get("body_names"))
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let positions: Vec<Value> = occurrences
                .iter()
                .filter(|o| o.get("component").and_then(Value::as_u64) == Some(id))
                .map(|o| {
                    json!({
                        "name": o.get("name"),
                        "position": translation(o.get("transform")),
                        "grounded": o.get("grounded"),
                    })
                })
                .collect();
            parts.push(json!({
                "component": id,
                "name": component.get("name").and_then(Value::as_str).unwrap_or(""),
                "part_number": string("part_number"),
                "description": string("description"),
                "revision": string("revision"),
                "file": import.and_then(|f| f.get("file")).and_then(Value::as_str).unwrap_or(""),
                "link": links
                    .get(&id.to_string())
                    .and_then(|l| l.get("path"))
                    .and_then(Value::as_str)
                    .unwrap_or(""),
                "bodies": bodies,
                "occurrences": positions.len(),
                "positions": positions,
            }));
        }

        let parts_total: usize = parts
            .iter()
            .map(|p| p.get("occurrences").and_then(Value::as_u64).unwrap_or(0) as usize)
            .sum();
        json!({
            "name": self.name(),
            "format": doc.get("format").and_then(Value::as_str).unwrap_or(""),
            "units": doc.get("units").and_then(Value::as_str).unwrap_or(""),
            "components": parts.len(),
            "occurrences": parts_total,
            "parts": parts,
        })
    }

    /// Resumo para o `/api/open`: o que o `.sc` diz de si mesmo.
    pub fn summary(&self) -> Value {
        let tree = self.tree();
        json!({
            "name": self.name(),
            "format": self.document.get("format"),
            "units": self.document.get("units"),
            "components": tree.get("components"),
            "occurrences": tree.get("occurrences"),
            "parameters": self.document.get("params").map(|p| p.as_object().map(|o| o.len()).unwrap_or(0)),
        })
    }
}

/// Uma definição da montagem, com o STEP que o `.sc` guarda e as posições.
pub struct Piece {
    pub part_number: String,
    pub name: String,
    pub blob: String,
    pub positions: Vec<[f64; 3]>,
}

impl Design {
    /// As peças importadas, uma por definição, com as ocorrências de cada uma.
    pub fn pieces(&self) -> Vec<Piece> {
        let doc = &self.document;
        let components = doc
            .get("components")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let occurrences = doc
            .get("occurrences")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let features = doc
            .get("features")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        let mut pieces = Vec::new();
        for feature in features.iter() {
            if feature.get("type").and_then(Value::as_str) != Some("import") {
                continue;
            }
            let Some(blob) = feature.get("step_blob").and_then(Value::as_str) else {
                continue;
            };
            if blob.is_empty() {
                continue;
            }
            let id = feature
                .get("component")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let component = components
                .iter()
                .find(|c| c.get("id").and_then(Value::as_u64) == Some(id));
            let part_number = component
                .and_then(|c| c.get("metadata"))
                .and_then(|m| m.get("part_number"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let name = component
                .and_then(|c| c.get("name"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let positions = occurrences
                .iter()
                .filter(|o| o.get("component").and_then(Value::as_u64) == Some(id))
                .map(|o| translation_value(o.get("transform")))
                .collect();
            pieces.push(Piece {
                part_number,
                name,
                blob: blob.to_string(),
                positions,
            });
        }
        pieces
    }
}

/// A translação de uma ocorrência, em mm.
fn translation_value(transform: Option<&Value>) -> [f64; 3] {
    let Some(matrix) = transform.and_then(Value::as_array) else {
        return [0.0; 3];
    };
    let mut out = [0.0f64; 3];
    for (axis, row) in matrix.iter().take(3).enumerate() {
        if let Some(value) = row
            .as_array()
            .and_then(|r| r.get(3))
            .and_then(Value::as_f64)
        {
            out[axis] = (value * 1000.0).round() / 1000.0;
        }
    }
    out
}

/// A translação de uma matriz 4×4 (linha 3, coluna 3 nas últimas três).
fn translation(transform: Option<&Value>) -> Value {
    json!(translation_value(transform))
}
