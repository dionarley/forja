# AGENTS.md — trabalhando na Forja

Monorepo da plataforma. Leia `docs/STYLE.md` (nomenclatura e estilo) e
`docs/ARCH.md` (como as peças se encaixam) antes de mexer.

## Regras duras

- **`apps/` é código de terceiro.** Não reformate, não renomeie, não “melhore” o
  estilo. O patch tem que ser mínimo e justificado no `FORJA-FORK.md` do fork.
- **Nunca abra PR nas origens.** Mudança em dependência: commit na branch `forja`
  do fork (`dionarley/<app>`) e bump do ponteiro do submodule aqui.
- **`apps/_gpl/` não entra em template nem em produto distribuído** (GPL-3.0).
- **Docs são fonte única.** Mudança de conteúdo/front matter em documento
  controlado = revisão + ECO; não edite PDF em `dist/`.
- **Segredos**: `credentials.txt` do TeamCraft ou `~/.config/forja/`. Nunca no
  repo, nunca em unit do systemd, nunca em log.

## Comandos

```sh
scripts/sync.sh                                   # submodules (solidcore, open-pdm)
tools/build.sh start                              # build com log, progresso e guarda de RAM
tools/build.sh watch                              # crate atual, RSS, ETA, pico
cargo build                                       # build cru (sem log nem guarda)
cargo fmt --check && cargo clippy -- -D warnings
cargo run -p forja-pdm -- health                  # TeamCraft + vault
cargo run -p forja-pdm -- tess ../PS-001.step     # relatório do kernel
tools/start-all.sh && tools/status.sh

# serviço de usuário (sobrevive a reboot; hoje o start/stop é por PID)
cp services/forja-pdm.service ~/.config/systemd/user/
systemctl --user daemon-reload && systemctl --user enable --now forja-pdm.service
```

Build: `incremental = false` de propósito (o kernel é grande; incremental gasta
GB sem ganho). O primeiro build leva ~25 min; os seguintes, segundos.

Use `tools/build.sh start` (não `cargo build` na mão): ele grava o log com hora,
mostra progresso/ETA e mata o build se a soma de `rustc` passar de
`FORJA_BUILD_MEM_MB`. Sem log, um build de 25 min é indistinguível de um travado.

Esta máquina tem 4 núcleos e 11 GB **sem swap**: `.cargo/config.toml` fixa
`jobs = 1`. Um `rustc` do kernel chega a ~4 GB; com 2 em paralelo a soma passou
de 7 GB e o sistema travou duas vezes. Não suba `jobs` sem medir o pico
(`ps -eo rss,comm`) e sem swap.

Submodule não materializado fica como diretório vazio em `apps/` (não clone sem
querer): só `solidcore` e `open-pdm` são necessários para compilar. Os outros
entram com `scripts/sync.sh --all`.

## Onde cada coisa mora

| quero mudar | vou em |
|---|---|
| como o STEP vira malha | `crates/forja-pdm/src/kernel.rs` |
| o handler de STEP do OpenPDM | `apps/open-pdm` (fork `forja`) + `crates/forja-pdm/src/handlers.rs` |
| a malha/STEP do kernel | `apps/solidcore` (fork `forja`) |
| o visualizador web | `crates/forja-pdm/web/index.html` |
| rotas e HTTP | `crates/forja-pdm/src/server.rs` |
| TeamCraft API e vault | `crates/forja-pdm/src/teamcraft.rs` |
| nomenclatura/estilo | `docs/STYLE.md` |

## Antes de dizer “pronto”

- `cargo build` sem warnings novos e `cargo fmt --check` limpo.
- Testou com um arquivo real do vault (`cargo run -p forja-pdm -- tess <sha>`),
  não só com fixture pequena.
- Se mexeu em dependência: commit no fork **e** bump do submodule, ambos pushados.
