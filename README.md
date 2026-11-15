# Forja

<p><img src="branding/logo/lockup.svg" alt="Forja — apps · serviço · pdm" height="72"></p>

Plataforma de desenvolvimento de produto da **Tessera**: os apps de CAD, vetor,
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
    forja-pdm.service  unit do systemd para o serviço
  tools/           subir tudo e ver as portas
  scripts/         sync.sh (submodules) e service.sh (start/stop do forja-pdm)
  docs/            STYLE.md (nomenclatura e estilo) e ARCH.md
  branding/        marca da plataforma: símbolo, wordmark, tokens
```

## Rodar

```sh
scripts/sync.sh                     # materializa solidcore + open-pdm (path deps)
tools/build.sh start                # serviço (padrão: -p forja-pdm), em segundo plano
tools/build.sh watch                # acompanha ao vivo
scripts/service.sh start            # 127.0.0.1:8090
tools/status.sh                     # as cinco portas, uma linha cada
```

O build compila o kernel inteiro na primeira vez (~25 min, com `jobs = 1`).
O limite vem de `.cargo/config.toml`: a máquina não tem swap e um único `rustc`
do kernel chega a ~6 GB — dois em paralelo estouram a RAM e travam o sistema.
Depois disso, mudança em `crates/` recompila em segundos.

### Acompanhar o build

`tools/build.sh start` compila em segundo plano com log completo: cada linha do
cargo chega com a hora, então o tempo de cada crate sai do próprio log.

| comando | o que mostra |
|---|---|
| `tools/build.sh status` | retrato: crate atual, tempo nele, RSS, RAM livre, pico, progresso e ETA |
| `tools/build.sh watch` | o mesmo, atualizando a cada 2 s |
| `tools/build.sh log -n 40` / `-f` | saída do cargo, com hora, do fim ou ao vivo |
| `tools/build.sh errors` | só `error`/`warning` |
| `tools/build.sh stop` | mata o build inteiro (grupo de processos) |
| `tools/build.sh history` | builds anteriores: resultado, tempo, pico de RAM |
| `tools/build.sh timings --open` | gantt do cargo (`target/cargo-timings`), por unidade |

O `status` também funciona para um build iniciado na mão (`cargo build`): ele lê
os processos do repo, mas aí não há log, progresso nem pico.

A guarda de memória é a mesma que travava a máquina antes: se a soma de
`rustc`/`cargo`/`cc`/`ld` passar de `FORJA_BUILD_MEM_MB` (padrão 9000 MB), o build
é morto e o motivo fica no log. Estado em `.run/build/` (fora do git).

Para o serviço voltar sozinho depois de um reboot:

```sh
cp services/forja-pdm.service ~/.config/systemd/user/
systemctl --user daemon-reload && systemctl --user enable --now forja-pdm.service
```

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

## Marcas

| marca | onde | papel |
|---|---|---|
| **Tessera** | `dionarley/tessera` (privado) | assina contrato, laudo e proposta; dona das cores |
| **Forja** | este repo | a bancada (apps + serviço + PDM) |
| **Gabarito** | `dionarley/gabarito` | molde de produto, agnóstico |
| **Brasa** | `diy_hot_gun` | primeiro produto (PS-001); marca própria |

A Forja não usa a marca de nenhum produto: `branding/` tem o símbolo da
plataforma (bloco com a boca do forno) e os tokens vêm da Tessera.

## Licenças

Os apps do caminho principal são Apache-2.0 (solidcraft, solidcore, cadcraft,
vectorcraft, gridcraft), MIT (cad-viewer) e MIT OR Apache-2.0 (open-pdm — o
fork ganhou `LICENSE-*`, que faltavam no upstream). O nosso código é
MIT OR Apache-2.0.

**OpenCADStudio é GPL-3.0** e por isso vive em `apps/_gpl/`: pode ser usado como
ferramenta, não entra no `Gabarito` nem em produto distribuído.
