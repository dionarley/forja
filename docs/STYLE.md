# STYLE — nomenclatura e estilo da Forja

Vale para o que **nós** escrevemos (`crates/`, `tools/`, `scripts/`, docs).
Código de terceiro em `apps/` mantém o estilo do upstream: lá só entra o patch
mínimo e o `FORJA-FORK.md`.

## 1. Nomes

| namespace | regra | exemplo |
|---|---|---|
| repositório | kebab-case, EN ou slug da marca | `forja`, `gabarito`, `brasa` |
| diretório de produto | slug da marca | `brasa` |
| crate Rust | `forja-<domínio>` | `forja-pdm`, `forja-core` |
| binário | igual ao crate | `forja-pdm` |
| módulo | substantivo singular, sem `utils`/`helpers` | `kernel`, `teamcraft`, `server` |
| branch | `main` tronco; `feat/`, `fix/`, `docs/`, `chore/`; fork = `forja` | `feat/medicao` |
| tag | `v<semver>` por repo | `v0.1.0` |
| variável de ambiente | `FORJA_<ESCOPO>_<CHAVE>` | `FORJA_TEAMCRAFT_URL` |
| rota HTTP | recurso no plural, sem nome de backend | `/api/items`, `/api/mesh` |
| item PDM | `<PROJ>-<TIPO>-<NNN>`, sempre 3 dígitos | `PS-001-LM-001` |
| documento | arquivo EN, conteúdo PT | `STYLE.md`, `ARCH.md` |
| script | `<função>_<objeto>`, sem nome de fornecedor | `eco/CHG-000005.py` |
| token de marca | `--brand-<token>` | `--brand-steel` |

Regra de ouro: **identificador em inglês, texto em português**, unidade SI no
nome do parâmetro (`flow_rate_m3_s`, `chord_mm`).

## 2. Rust

- edition 2024 nos crates novos, MSRV 1.88 (o do kernel).
- `unsafe_code = "forbid"`.
- Bibliotecas: `thiserror` e sem `unwrap`/`expect`/`panic`/indexação direta
  (clippy `deny`). Binários e serviços: `anyhow` nas bordas.
- `//!` em português no topo do módulo, explicando **por que** ele existe;
  `///` no item público.
- `cargo fmt` e `cargo clippy -- -D warnings` antes de comitar.
- Teste de unidade no módulo; integração em `tests/`; teste com arquivo real
  guarda o SHA-256 do arquivo no próprio teste.

## 3. Python

3.11+, `uv`, `ruff` (100 colunas), type hints, `pytest`. Script só age quando
chamado: importar não escreve nada.

## 4. Front (site, visualizador)

TypeScript quando houver; CSS com os tokens de marca (`--brand-*`); nada de
CDN em produção quando der para empacotar.

## 5. Commits

```
tipo: resumo no imperativo

por que, em uma ou duas frases, se não for óbvio.
```

`tipo` ∈ `feat`, `fix`, `docs`, `test`, `refactor`, `chore`, `perf`. Português,
sem emoji, sem ponto final no resumo.

## 6. Versionamento

| coisa | esquema |
|---|---|
| plataforma (este repo) | semver `0.x.y` |
| produto | revisão **A, B, C…** + ECO; nunca tag semver |
| template (Gabarito) | semver |
| marca | versão de tokens (`brand v1`) |

## 7. Dependências

Submodule pinado por commit; `scripts/sync.sh` para atualizar; `Cargo.lock` e
`uv.lock` commitados. Mudança em dependência: commit no fork → bump aqui.
Nenhum PR nas origens.

## 8. Documentação e ECO

`docs/` é fonte única. Mudar conteúdo ou front matter de documento controlado =
revisão nova + ECO registrada no PDM, com o PDF liberado em `dist/`.

## 9. Segredos

`~/.config/forja/credentials.toml` (0600) ou o `credentials.txt` do TeamCraft.
Nunca em repositório, nunca em unit do systemd, nunca em log.

## 10. Processos e portas

Processos nossos: `forja-*` (`forja-pdm`, `forja-docs`). Apps de terceiro
mantêm o nome (`teamcraft`, `gridcraft`, `solidcraft`).

| porta | quem |
|---|---|
| 8080 | TeamCraft |
| 8081 | docs (Docusaurus) |
| 8088 | GridCraft |
| 8089 | visualizador DXF |
| 8090 | forja-pdm |
