# ARCH — como as peças se encaixam

## Camadas

```
produto (repo próprio, marca própria)      ex.: Brasa / PS-001
    │  usa
    ▼
gabarito (molde, agnóstico)                docs, BOM, ECO, brand/, CI
    │  depende da
    ▼
forja (esta plataforma)                    apps + crates + tools
    │  é dona de
    ▼
apps/ (forks) + crates/ (nosso código)
```

O produto **não** importa código da Forja: ele usa as ferramentas (CLI, serviço,
apps). Isso permite que dois produtos usem a mesma bancada sem herdar marca nem
versionamento um do outro.

## Serviço PDM (`crates/forja-pdm`)

```
TeamCraft ──API──▶ vault/objects/<sha>  ──bytes──▶ HandlerRegistry (open-pdm-core)
                                                        │ with_first(SolidStepHandler)
                                                        ▼
                                          p21::parse → topology::read_solids_with
                                                 (SurfaceTrims: pcurves por projeção)
                                                        ▼
                                  solidcore_tess::plan → TessPlan::evaluate
                                                        ▼
                                      Mesh { positions f32, indices u32 }
```

- **TeamCraft** é a fonte de verdade do PDM (itens, revisões, ECO, arquivos).
- **open-pdm-core** dá repositório git, índice e a trait `CadHandler`. O único
  patch no fork é `HandlerRegistry::with_first`, que põe o nosso handler de STEP
  na frente dos embutidos.
- **solidcore** faz o trabalho de geometria: lê o STEP, tessela cada corpo com
  tolerância proporcional ao tamanho (1/1000 da diagonal, mínimo 1 µm, 20° por
  faceta) e devolve a malha.
- **`SurfaceTrims`** completa o leitor: onde o kernel só tem pcurve de forma
  fechada (uma aresta NURBS sobre superfície spline não tem), o serviço projeta a
  aresta na superfície e entrega a polilinha em (u, v). Sem isso o leitor recusa
  o corpo inteiro — era o que sumia com 18 dos 22 corpos do PS-001.
- **Cache** por conteúdo (SHA-256 no TeamCraft, oid do blob no git): reabrir a
  mesma revisão não repete a tesselação.

### O que o serviço não faz

- Não mede volume exato, área nem massa: publica o volume da malha (teorema da
  divergência, dentro de 0,1% do valor do app no PS-001); o número exato do
  B-rep e a massa são do `solidcraft-cli eval`, que pode ser chamado sob demanda.
- Não substitui o TeamCraft: não cria item, não libera revisão.
- Não é editor: abre, mostra e entrega a malha.

## Escolhas

| escolha | por quê |
|---|---|
| `tiny_http` em vez de axum/tokio | o trabalho é CPU (parse + tesselação); thread por requisição basta e não puxa runtime assíncrono |
| `ureq` sem TLS | TeamCraft é HTTP na máquina local |
| `solidcore` linkado, não CLI | o CLI do app empacotado leva ~69 s por arquivo (mede massa e exporta em tolerância fina); linkado, a mesma malha sai em ~4 s |
| submodules | fork de verdade no GitHub, monorepo leve, clone sob demanda |
| GPL separado em `apps/_gpl` | não contamina o template comercial |
