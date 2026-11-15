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
cargo build                                       # crates/forja-pdm
cargo fmt --check && cargo clippy -- -D warnings
cargo run -p forja-pdm -- health                  # TeamCraft + vault
cargo run -p forja-pdm -- tess ../PS-001.step     # relatório do kernel
tools/start-all.sh && tools/status.sh
```

Build: `incremental = false` de propósito (o kernel é grande; incremental gasta
GB sem ganho). O primeiro build leva ~25 min; os seguintes, segundos.

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
