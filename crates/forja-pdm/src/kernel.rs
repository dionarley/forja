//! STEP do TeamCraft aos triângulos, com o kernel do `solidcore`.
//!
//! O caminho é o de qualquer aplicação do kernel: [`p21::parse`] lê a estrutura de
//! troca, [`topology::read_solids`] transforma os sólidos em registros B-rep,
//! [`solidcore_tess::plan`] planeja as faces em (u, v) e [`TessPlan::evaluate`]
//! avalia o plano em uma [`Mesh`]. A malha sai no formato do OpenPDM
//! ([`open_pdm_core::Mesh`]: `positions` f32 e `indices` u32), que é o mesmo
//! `BufferGeometry` do visualizador.

use std::time::Instant;

use anyhow::{anyhow, Context, Result};
use open_pdm_core::Mesh as CoreMesh;
use solidcore_record::brep::Brep;
use solidcore_record::step::p21;
use solidcore_step::topology;
use solidcore_tess::MeshTolerance;

/// Faixa de um corpo, em mm.
#[derive(Clone, Debug)]
pub struct Bounds {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

impl Bounds {
    fn of(b: &Brep) -> Option<Self> {
        let mut min = [f64::INFINITY; 3];
        let mut max = [f64::NEG_INFINITY; 3];
        for v in &b.vertices {
            for (i, c) in v.point.iter().enumerate() {
                if let (Some(lo), Some(hi)) = (min.get_mut(i), max.get_mut(i)) {
                    *lo = lo.min(*c);
                    *hi = hi.max(*c);
                }
            }
        }
        if min.iter().any(|c| !c.is_finite()) || max.iter().any(|c| !c.is_finite()) {
            return None;
        }
        Some(Bounds { min, max })
    }

    fn union(&mut self, other: &Bounds) {
        for i in 0..3 {
            if let (Some(lo), Some(hi)) = (self.min.get_mut(i), self.max.get_mut(i)) {
                *lo = lo.min(other.min.get(i).copied().unwrap_or(*lo));
                *hi = hi.max(other.max.get(i).copied().unwrap_or(*hi));
            }
        }
    }

    fn diagonal(&self) -> f64 {
        let d = [
            self.max[0] - self.min[0],
            self.max[1] - self.min[1],
            self.max[2] - self.min[2],
        ];
        (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
    }

    fn json(&self) -> serde_json::Value {
        serde_json::json!({ "min": self.min, "max": self.max })
    }
}

/// Um corpo do arquivo e o que a tesselação dele produziu.
#[derive(Clone, Debug)]
pub struct BodyReport {
    pub name: String,
    pub item: u64,
    pub product: Option<String>,
    pub sheet: bool,
    pub faces: usize,
    pub edges: usize,
    pub vertices: usize,
    pub triangles: usize,
    pub refused_faces: usize,
    pub unmeshed_faces: usize,
    pub notes: usize,
    pub bounds: Option<Bounds>,
}

impl BodyReport {
    pub fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "name": self.name,
            "item": self.item,
            "product": self.product,
            "sheet": self.sheet,
            "faces": self.faces,
            "edges": self.edges,
            "vertices": self.vertices,
            "triangles": self.triangles,
            "refused_faces": self.refused_faces,
            "unmeshed_faces": self.unmeshed_faces,
            "notes": self.notes,
            "bounds": self.bounds.as_ref().map(Bounds::json),
        })
    }
}

/// A malha inteira do arquivo, com o relatório de cada corpo.
#[derive(Clone, Debug)]
pub struct KernelMesh {
    pub mesh: CoreMesh,
    pub bodies: Vec<BodyReport>,
    pub failures: Vec<String>,
    pub bounds: Option<Bounds>,
    pub elapsed_ms: u128,
}

impl KernelMesh {
    pub fn faces(&self) -> usize {
        self.bodies.iter().map(|b| b.faces).sum()
    }

    pub fn triangles(&self) -> usize {
        self.mesh.triangle_count()
    }

    pub fn report(&self) -> serde_json::Value {
        serde_json::json!({
            "kernel": "solidcore",
            "elapsed_ms": self.elapsed_ms,
            "bodies": self.bodies.iter().map(BodyReport::json).collect::<Vec<_>>(),
            "failures": self.failures,
            "bounds": self.bounds.as_ref().map(Bounds::json),
            "size_mm": self.bounds.as_ref().map(Bounds::diagonal),
            "faces": self.faces(),
            "triangles": self.triangles(),
            "vertices": self.mesh.vertex_count(),
        })
    }
}

/// Tessela um STEP (bytes) para a malha de exibição.
///
/// Cada corpo recebe a tolerância de exibição proporcional ao próprio tamanho
/// (1/1000 da diagonal, mínimo 1 µm), como qualquer viewport do kernel: peças
/// grandes e pequenas no mesmo arquivo saem na mesma qualidade aparente.
pub fn tessellate(bytes: &[u8]) -> Result<KernelMesh> {
    let started = Instant::now();
    let text = std::str::from_utf8(bytes).context("o STEP não é texto UTF-8")?;
    let exchange = p21::parse(text).map_err(|e| anyhow!("STEP recusado pelo leitor: {e}"))?;

    let solids = topology::read_solids(&exchange);
    if solids.solids.is_empty() {
        let refused: Vec<String> = solids
            .failures
            .iter()
            .map(|f| format!("#{}: {}", f.item, f.message))
            .collect();
        return Err(anyhow!(
            "nenhum corpo sólido legível no STEP{}",
            if refused.is_empty() {
                String::new()
            } else {
                format!(" ({})", refused.join("; "))
            }
        ));
    }

    let mut mesh = CoreMesh::default();
    let mut bodies = Vec::with_capacity(solids.solids.len());
    let mut all = None::<Bounds>;

    for solid in &solids.solids {
        let bounds = Bounds::of(&solid.brep);
        let size = bounds.as_ref().map(Bounds::diagonal).filter(|d| *d > 1e-9);
        let tolerance = match size {
            Some(size) => MeshTolerance::display_of(size),
            None => MeshTolerance::display(0.05),
        };
        let size = size.unwrap_or(100.0);

        let (plan, stats) = solidcore_tess::plan(&solid.brep, &tolerance, size)
            .map_err(|e| anyhow!("corpo #{} ({}): tesselação recusada: {e}", solid.item, solid.name))?;
        let (body_mesh, refused) = plan.evaluate();

        let base = u32::try_from(mesh.positions.len() / 3)
            .map_err(|_| anyhow!("malha grande demais para índices de 32 bits"))?;
        for p in &body_mesh.positions {
            mesh.positions.push(p.x as f32);
            mesh.positions.push(p.y as f32);
            mesh.positions.push(p.z as f32);
        }
        for t in &body_mesh.triangles {
            for i in t {
                mesh.indices.push(base + i);
            }
        }

        let refused_faces = stats.unmeshed.len() + refused.len();
        if let Some(b) = &bounds {
            all.get_or_insert(b.clone()).union(b);
        }

        bodies.push(BodyReport {
            name: solid.name.clone(),
            item: solid.item,
            product: solid.product.clone(),
            sheet: solid.sheet,
            faces: solid.brep.faces.len(),
            edges: solid.brep.edges.len(),
            vertices: solid.brep.vertices.len(),
            triangles: body_mesh.triangles.len(),
            refused_faces,
            unmeshed_faces: stats.unmeshed.len(),
            notes: solid.notes.len(),
            bounds,
        });
    }

    let failures = solids
        .failures
        .iter()
        .map(|f| format!("#{}: {}", f.item, f.message))
        .collect::<Vec<_>>();

    Ok(KernelMesh {
        mesh,
        bodies,
        failures,
        bounds: all,
        elapsed_ms: started.elapsed().as_millis(),
    })
}

/// Relatório enxuto de um arquivo, para a linha de comando.
pub fn describe(bytes: &[u8]) -> Result<serde_json::Value> {
    let km = tessellate(bytes)?;
    let mut report = km.report();
    if let Some(obj) = report.as_object_mut() {
        obj.insert(
            "mesh".into(),
            serde_json::json!({
                "vertices": km.mesh.vertex_count(),
                "triangles": km.mesh.triangle_count(),
                "bytes_bin": km.mesh.positions.len() * 4 + km.mesh.indices.len() * 4,
            }),
        );
    }
    Ok(report)
}
