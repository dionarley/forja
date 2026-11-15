//! O handler de STEP do serviço: o do OpenPDM só lê o cabeçalho e delega a
//! tesselação ao cliente (OpenCASCADE em WebAssembly); aqui o kernel do
//! `solidcore` tessela no servidor e a malha já sai pronta.
//!
//! É o ponto onde os dois projetos se encontram: a trait [`CadHandler`] do
//! OpenPDM, implementada com o kernel B-rep do solidcore, registrada antes dos
//! handlers embutidos por `HandlerRegistry::with_first` (a emenda da branch
//! `forja` do fork do OpenPDM).

use anyhow::anyhow;
use open_pdm_core::handlers::step::StepHandler;
use open_pdm_core::{CadHandler, FileMetadata, HandlerRegistry, Mesh, Result as PdmResult, ShapePayload};

use crate::kernel;

/// STEP com tesselação pelo solidcore.
pub struct SolidStepHandler;

impl CadHandler for SolidStepHandler {
    fn name(&self) -> &'static str {
        "step-solidcore"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["step", "stp"]
    }

    fn metadata(&self, path: &str, bytes: &[u8]) -> PdmResult<FileMetadata> {
        // Cabeçalho, sistema de origem e unidades: o que o handler embutido já lê.
        let mut metadata = StepHandler.metadata(path, bytes)?;
        metadata.attributes.insert("kernel".into(), "solidcore".into());
        metadata
            .attributes
            .insert("tessellation".into(), "solidcore".into());
        Ok(metadata)
    }

    fn mesh(&self, path: &str, bytes: &[u8]) -> PdmResult<Option<Mesh>> {
        match kernel::tessellate(bytes) {
            Ok(tessellated) => Ok(Some(tessellated.mesh)),
            Err(error) => Err(open_pdm_core::Error::parse("STEP", format!("{path}: {error}"))),
        }
    }

    fn shape_payloads(&self, _path: &str, _bytes: &[u8]) -> PdmResult<Vec<ShapePayload>> {
        // Nada a mandar para o cliente: a geometria exata já virou malha aqui.
        Ok(Vec::new())
    }
}

/// O registro do serviço: o handler do kernel na frente de todos.
pub fn registry() -> HandlerRegistry {
    HandlerRegistry::with_first(Box::new(SolidStepHandler))
}

/// Metadados de um arquivo pelo registro do serviço, mais o nome do handler.
pub fn describe(name: &str, bytes: &[u8]) -> anyhow::Result<(FileMetadata, &'static str)> {
    let registry = registry();
    let handler = registry.resolve(name);
    let metadata = handler
        .metadata(name, bytes)
        .map_err(|e| anyhow!("metadados de {name}: {e}"))?;
    Ok((metadata, handler.name()))
}
