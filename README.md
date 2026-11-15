# Forja

Plataforma de desenvolvimento de produto da **Dionarley**: os apps de CAD, vetor,
planilha e PDM em um só lugar, com os forks que a gente realmente modifica.

> A Forja é a bancada. O **Gabarito** é o molde de cada produto. O produto tem
> marca própria (o **Brasa** é a primeira).

## O que tem dentro

```
forja/
  apps/            forks (submodules) — cada um é um repo no GitHub
    solidcraft/    CAD paramétrico + kernel
    solidcore/     kernel B-rep: lê STEP, tessela, mede
    cadcraft/      CAD
    vectorcraft/   desenho vetorial
    gridcraft/     planilha
    cad-viewer/    visualizador DXF (MLightCAD)
    open-pdm/      core PDM: git + índice + handlers
    _gpl/opencadstudio/   CAD 2D/3D — GPL-3.0, fora do template
  crates/
    forja-pdm/     serviço PDM: abre STEP do TeamCraft com o kernel
  services/
    teamcraft/     o PDM que roda na máquina (binário, não é fork)
  tools/           subir/parar tudo, portas
  docs/            STYLE.md (nomenclatura e estilo) e ARCH.md
  branding/        identidade da plataforma
```

## Rodar

```sh
scripts/sync.sh                     # inicializa/atualiza os submodules necessários
cargo build                         # crates/forja-pdm
tools/start-all.sh                  # sobe o que falta e mostra as portas
```

O build compila o kernel inteiro na primeira vez (~25 min) e usa `jobs = 2`
(`.cargo/config.toml`): a máquina não tem swap e paralelismo maior trava o
sistema. Depois disso, mudança em `crates/` recompila em segundos.

Portas: `8080` TeamCraft · `8081` docs · `8088` GridCraft · `8089` visualizador
DXF · `8090` forja-pdm.

O `forja-pdm` precisa de `apps/solidcore` e `apps/open-pdm` materializados
(são *path deps*):

```sh
git submodule update --init --depth 1 apps/solidcore apps/open-pdm
```

Os demais ficam só pinados — `git submodule update --init apps/solidcraft` no dia
em que for mexer nele.

## Forks, não PRs

Cada app é um fork em `dionarley/<app>`, branch **`forja`**. Mudança de
dependência = commit na `forja` do fork + bump do ponteiro do submodule aqui.
Nunca PR na origem.

```sh
cd apps/solidcore && git checkout forja   # edite, comite, git push
cd ../.. && git add apps/solidcore && git commit -m "chore: atualiza solidcore"
```

Cada fork tem um `FORJA-FORK.md` com o que muda e como rebasear do upstream.

## Licenças

Os apps do caminho principal são Apache-2.0 (solidcraft, solidcore, cadcraft,
vectorcraft, gridcraft), MIT (cad-viewer) e MIT OR Apache-2.0 (open-pdm — o
fork ganhou `LICENSE-*`, que faltavam no upstream). O nosso código é
MIT OR Apache-2.0.

**OpenCADStudio é GPL-3.0** e por isso vive em `apps/_gpl/`: pode ser usado como
ferramenta, não entra no `Gabarito` nem em produto distribuído.
