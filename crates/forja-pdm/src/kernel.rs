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
use solidcore::eval::curve::CurveEval;
use solidcore::eval::surface::SurfaceEval;
use solidcore::eval::Starts;
use solidcore_convert as convert;
use solidcore_ops::pcurve::{TrimFace, Trims};
use solidcore_record::brep::{Brep, Curve2};
use solidcore_record::step::p21;
use solidcore_record::Vec3;
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
    /// Volume do corpo fechado, pela malha (teorema da divergência).
    pub volume_mm3: f64,
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
            "volume_mm3": self.volume_mm3,
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

/// Faz as pcurves que o STEP não traz, projetando cada aresta na superfície da face.
///
/// O leitor só aceita o corpo inteiro quando todas as faces têm pcurve, e o kernel
/// só conhece as de forma fechada: uma curva NURBS sobre superfície spline não tem
/// nenhuma, e o corpo era recusado — foi assim que 18 dos 22 corpos do PS-001
/// sumiam. Aqui cada aresta é amostrada, cada ponto é projetado no (u, v) da face
/// e a curva sai como polilinha, refinada até a linha assentar na aresta dentro de
/// 1/4 da tolerância da face.
struct SurfaceTrims;

impl Trims for SurfaceTrims {
    fn trims(&self, face: &TrimFace<'_>) -> std::result::Result<Vec<Vec<Curve2>>, String> {
        // O leitor guarda as pcurves na forma NURBS da face quando ela tem uma
        // (`pcurve::pcurves_within(&surface, ...)`), e é nela que `raise_edges` as
        // confere. O rótulo analítico tem outros parâmetros: projetar nele dava
        // pcurves a milímetros da aresta.
        let surface = convert::surface(face.nurbs.unwrap_or(face.surface)).map_err(|e| e.to_string())?;
        let on = SurfaceEval::new(&surface).map_err(|e| e.to_string())?;
        let width = face.domain[0][1] - face.domain[0][0];
        // Uma grade de partidas por face: `SurfaceEval::project` a reconstrói a cada
        // chamada, e uma pcurve de 256 pontos pagava isso 256 vezes.
        let starts = Starts::default();
        let mut loops = Vec::with_capacity(face.loops.len());
        for uses in &face.loops {
            let mut pcs = Vec::with_capacity(uses.len());
            for u in uses {
                if let Some(pc) = &u.pcurve {
                    pcs.push(pc.clone());
                    continue;
                }
                let e = face
                    .edges
                    .get(u.edge as usize)
                    .ok_or("uso aponta para aresta que a face não tem")?;
                let curve = convert::curve(e.nurbs.unwrap_or(e.curve)).map_err(|m| m.to_string())?;
                let eval = CurveEval::new(&curve).map_err(|m| m.to_string())?;
                let target = face.tolerance.max(e.tolerance).max(1e-6);
                pcs.push(projected(&eval, &on, &starts, e.interval, target, width)?);
            }
            loops.push(pcs);
        }
        Ok(loops)
    }
}

/// A aresta `eval` no (u, v) de `on`, como polilinha nos parâmetros da aresta.
fn projected(
    eval: &CurveEval,
    on: &SurfaceEval,
    starts: &Starts,
    interval: [f64; 2],
    tolerance: f64,
    width: f64,
) -> std::result::Result<Curve2, String> {
    let [t0, t1] = interval;
    let accept = 4.0 * tolerance;
    // O que importa é o desvio da pcurve, que recorta a face em (u, v): 1 µm já é
    // muito abaixo do que a malha mostra, e uma tolerância de 1e-6 nunca fecha.
    let target = 0.25 * tolerance.max(4e-3);
    // O corpo é refeito do zero a cada dobra: 8 pontos costumam bastar numa reta,
    // e uma spline sobre spline pede até 256.
    let mut n = 8usize;
    loop {
        let mut knots = Vec::with_capacity(n + 3);
        let mut points = Vec::with_capacity(n + 1);
        let mut near: Option<[f64; 2]> = None;
        let mut last_u = 0.0;
        for i in 0..=n {
            let t = t0 + (t1 - t0) * i as f64 / n as f64;
            let p = eval.point(t).ok_or("aresta sem ponto no seu intervalo")?;
            // Com `accept` apertado uma aresta que assenta na face com folga de
            // micrômetros não projeta; quem julga a folga é o `raise_edges` do
            // leitor, então aqui basta o ponto mais próximo que a superfície tem.
            let uv = on
                .project_with(starts, p, near, accept)
                .or_else(|| on.project_with(starts, p, near, f64::INFINITY))
                .ok_or_else(|| format!("ponto da aresta em {t} não cai na superfície"))?;
            let mut uv = uv;
            if i > 0 && width > 0.0 {
                // Uma face periódica devolve o u do outro lado da emenda; traga de volta
                // para o lado do ponto anterior.
                while uv[0] - last_u > 0.5 * width {
                    uv[0] -= width;
                }
                while last_u - uv[0] > 0.5 * width {
                    uv[0] += width;
                }
            }
            last_u = uv[0];
            near = Some(uv);
            points.push(uv);
            knots.push(t);
        }
        // A polilinha tem que cair sobre a aresta nos pontos médios também.
        let mut worst: f64 = 0.0;
        for i in 0..n {
            let t = 0.5 * (knots[i] + knots[i + 1]);
            let Some(p) = eval.point(t) else { continue };
            let a = points[i];
            let b = points[i + 1];
            let mid = [0.5 * (a[0] + b[0]), 0.5 * (a[1] + b[1])];
            let Some(q) = on.point(mid) else { continue };
            worst = worst.max((q - p).length());
        }
        if worst <= target || n >= 256 {
            let mut knots = knots;
            knots.insert(0, t0);
            knots.push(t1);
            return Ok(Curve2::Spline { knots, points });
        }
        n *= 2;
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

    let solids = topology::read_solids_with(&exchange, &SurfaceTrims);
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
        // A faixa do brep (vértices das arestas) serve só para escolher a tolerância:
        // numa superfície varrida ela não é a do sólido. A faixa que se publica é a
        // da malha, que é a do sólido que se vê.
        let brep_bounds = Bounds::of(&solid.brep);
        let bounds = brep_bounds.clone();
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
        let (bounds, volume_mm3) = mesh_measure(&body_mesh.positions, &body_mesh.triangles);
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
            volume_mm3,
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

/// Faixa e volume de um corpo pela malha dele: o volume sai do teorema da
/// divergência sobre os triângulos (a origem pode ser qualquer uma).
fn mesh_measure(positions: &[Vec3], triangles: &[[u32; 3]]) -> (Option<Bounds>, f64) {
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for p in positions {
        for (i, c) in [p.x, p.y, p.z].into_iter().enumerate() {
            min[i] = min[i].min(c as f64);
            max[i] = max[i].max(c as f64);
        }
    }
    let bounds = (!positions.is_empty() && min.iter().chain(max.iter()).all(|c| c.is_finite()))
        .then_some(Bounds { min, max });

    let mut six_v = 0.0f64;
    for t in triangles {
        let (Some(a), Some(b), Some(c)) = (
            positions.get(t[0] as usize),
            positions.get(t[1] as usize),
            positions.get(t[2] as usize),
        ) else {
            continue;
        };
        let (a, b, c) = (
            [a.x as f64, a.y as f64, a.z as f64],
            [b.x as f64, b.y as f64, b.z as f64],
            [c.x as f64, c.y as f64, c.z as f64],
        );
        let cross = [
            b[1] * c[2] - b[2] * c[1],
            b[2] * c[0] - b[0] * c[2],
            b[0] * c[1] - b[1] * c[0],
        ];
        six_v += a[0] * cross[0] + a[1] * cross[1] + a[2] * cross[2];
    }
    (bounds, (six_v / 6.0).abs())
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
