# TeamCraft — o PDM que roda na máquina

Não é fork e não é nosso: é um binário instalado em `~/teamcraft/`, servido por
dois serviços de usuário do systemd. A Forja só consome a API.

```
~/.config/systemd/user/teamcraft.service          API em 127.0.0.1:8080
~/.config/systemd/user/teamcraft-db.service       banco em podman (teamcraft-db)
~/teamcraft/                                        instalação + credentials.txt
~/teamcraft/data/vault/objects/<xx>/<yy>/<sha>     arquivos por conteúdo (SHA-256)
```

## Operar

```sh
systemctl --user status teamcraft.service teamcraft-db.service
systemctl --user restart teamcraft.service
journalctl --user -u teamcraft.service -n 50 --no-pager
```

O `teamcraft.service` depende do banco (`Requires=` + `After=`), então reiniciar
o banco derruba a API junto e o systemd levanta as duas.

## API que o forja-pdm usa

| chamada | para quê |
|---|---|
| `GET /api/v1/items?project=PS-001` | lista de itens do projeto |
| `GET /api/v1/items/<n>` | item + revisões |
| `GET /api/v1/items/<n>/revisions/<rev>/files` | arquivos da revisão (nome, sha256, tamanho) |

O download não passa pela API: o `forja-pdm` lê o objeto do vault pelo sha256
(`~/teamcraft/data/vault/objects/<2>/<2>/<sha256>`), já que a API entrega o mesmo
arquivo por `/api/v1/file-versions/<id>/content`.

Rotas que o `forja-pdm` publica em `127.0.0.1:8090` (a UI é `/`):

| chamada | para quê |
|---|---|
| `GET /api/items?project=PS-001` | itens do projeto (`/api/teamcraft/items` é apelido antigo) |
| `GET /api/items/<n>` | item + revisões |
| `GET /api/items/<n>/revisions/<rev>/files` | arquivos da revisão |
| `GET /api/tree?code=&rev=` | árvore da montagem lida do `.sc` nativo, com material e massa do PDM |
| `GET /api/thumb?code=&rev=` | miniatura que o SolidCraft grava dentro do `.sc` da peça |
| `GET /api/assembly?code=&rev=` | a montagem inteira: cada peça nas suas posições (22 no PS-001) |
| `GET /api/open`, `/api/mesh`, `/api/raw` | metadados, malha FJPM e bytes do arquivo |
| `GET /healthz` | TeamCraft, token e vault |

O `.sc` é lido **sem converter**: `document.json` dá a estrutura (componentes,
links, ocorrências) e `cache/thumbnail.png` dá a miniatura. A geometria da malha
vem do STEP que a própria revisão guarda — o `.sc` não descreve malha.

Autenticação por bearer token: `admin_token` no `~/teamcraft/credentials.txt`.
O `forja-pdm` lê `FORJA_TEAMCRAFT_TOKEN`; sem ela, cai no `credentials.txt`.

## Regras

- **Token não sai da máquina**: nunca em repositório, nunca em unit do systemd
  (que é legível por qualquer processo do usuário), nunca em log.
- **Não apague objeto do vault**: o PDM referencia por SHA-256 e não há lixeira.
- **Backup antes de migração** de item/revisão (`tools/plm/teamcraft_backup.sh`
  no repo do produto) — o banco é o índice do vault, e um sem o outro não serve.
- Porta fixa em `127.0.0.1:8080`: o `start-all.sh` e o `.env` do MCP dependem
  dela. Trocar exige mexer nos dois.
