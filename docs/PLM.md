# PLM — o que a Forja aprende com o SOLIDWORKS PLM

Referência: página **SOLIDWORKS PLM** (solidworks.com/pt-br/product/solidworks-plm),
lida em 15/11/2026. Ela vende dez recursos, em três pacotes, sobre a plataforma
3DEXPERIENCE na nuvem.

A página é **referência limpa, não paridade**: serve de catálogo do problema que um
PLM resolve. A Forja não persegue os mesmos recursos nem o mesmo modelo de entrega —
o produto é **app local + serviço no servidor on-premise**, e o caminho é a
**integração com o SolidCraft**: a peça nasce e é editada no app, o serviço guarda,
versiona e mostra. Nada de nuvem multi-tenant.

## 1. PDM, PLM, ERP/MES — quem faz o quê

| camada | o que organiza | na Forja |
|---|---|---|
| **PDM** | dado técnico de projeto: peça, revisão, arquivo, BOM de engenharia, check-out | TeamCraft é o PDM; o `forja-pdm` é a janela e a camada de derivados |
| **PLM** | ciclo de vida e colaboração: ECO/ECR, prontidão de liberação, fornecedor, variante, portfólio | TeamCraft cobre hoje ECO/ECR, revisão e prontidão; o resto é fase futura |
| **ERP / MES** | execução: compra, estoque, roteiro, custo real, chão de fábrica | **fora de escopo**; no máximo a BOM sai daqui para alimentar um ERP |

A fronteira que vale: o PLM guarda **peça e montagem**; documento e código vivem no
git (STYLE §8). O que o ERP precisa é derivado — BOM, quantidade, massa — e não
volta para dentro do PDM.

## 2. Onde a Forja está (verificado em 16/11/2026)

| # | recurso da página | estado | onde está hoje |
|---|---|---|---|
| 1 | BOM multifuncional | **parcial** | TeamCraft tem BOM multinível (`get_bom`); o modelo já exporta BOM do conjunto (22 posições / 14 peças); o `forja-pdm` não publica BOM |
| 2 | Planejamento conectado ao CAD | **não existe** | — (adaptação: painel de pendências) |
| 3 | Matérias-primas | **parcial** | material e massa já existem no modelo (1.721 g); catálogo e massa calculada ainda não |
| 4 | Modelo digital 3D | **parcial** | 14 peças nativas + montagem de 22 posições no `.sc`; o visualizador ainda abre um arquivo só |
| 5 | Alterações formais | **parcial** | ECR/ECO e prontidão completos no TeamCraft; nada disso aparece no `forja-pdm` |
| 6 | Código-fonte no PLM | **fora de escopo** | código e documento são git; a revisão guarda no máximo o ponteiro |
| 7 | Classificação IP | **parcial** | `classification` da revisão existe; sem taxonomia nem busca por classe |
| 8 | Rede de fornecedores | **parcial** | só `unit_cost`; fornecedor e prazo não existem |
| 9 | Itens do fabricante (MBOM) | **não existe** | — |
| 10 | Variantes | **não existe** | — |

## 3. Fases

Cada fase entrega algo usável sozinha. Tamanho: **P** ≤ 1 sessão, **M** 2–3,
**G** mais que isso.

### Fase 0 — o modelo vira dado · feito

- 14 definições nativas (`PS-001-LM-01…14`) em `cad/solidcraft/parts/`.
- `PS-001.sc` virou montagem: **14 links, 22 posições** (as 8 repetidas por
  ocorrência, não por cópia de história).
- material por corpo, densidade e massa por peça; massa total **1.721 g**.
- STEP por peça e STEP do conjunto (assembly).

**Aceite:** volume do conjunto igual ao flat (448.434 mm³, diferença < 0,001%),
BOM do app idêntica ao `docs/construcao/BOM.csv` (1, 1, 1, 1, 1, 4, 4, 2, 2, 1, 1,
1, 1, 1 = 22 posições), e cada peça com `.sc` + STEP + material + massa.

### Fase 1 — SolidCraft ↔ PDM · M

É a fase que dá sentido às outras: o app deixa de ser um fim e passa a falar com o
serviço.

- **abrir do PDM**: escolher item/revisão no `forja-pdm` e abrir a peça no
  SolidCraft (arquivo do vault, não cópia solta).
- **check-out / check-in**: a revisão em `in_work` fica travada para o editor; ao
  salvar, o app devolve `.sc` + STEP e o serviço deriva volume, material e massa.
- **BOM da montagem**: o BOM que o SolidCraft já exporta (`assembly.bom`) vira a
  BOM da revisão do conjunto, com `part_number` e quantidade — sem digitação.
- **material e massa**: o que o app calcula preenche `material` e `mass_kg` da
  revisão; divergência com o valor declarado vira aviso.
- trabalho em lote (criar posições, extrair peças, exportar STEP) roda no
  `solidcraft-cli`, **não na janela**: a interface congela ao carregar um conjunto
  grande, e o lote não precisa de tela.

**Aceite:** uma peça sai do PDM, é editada no app e volta como revisão nova com
arquivo, STEP, massa e BOM, sem comando manual no meio.

### Fase 2 — matéria-prima e massa · P–M

- catálogo de materiais: densidade, custo/kg, classe (metal, polímero, madeira…).
  Base embutida no serviço + `materials.toml` do produto para overrides.
- massa calculada = volume da malha × densidade, **ao lado** da massa declarada;
  divergência acima de 2% vira aviso.
- `mass_kg` de cada peça preenchido na revisão nova (hoje é `null`).

**Aceite:** `PS-001` fecha a soma das peças e o conjunto declara a massa total;
nenhuma peça sem `material` ou `mass_kg`.

### Fase 3 — BOM e alterações · M

- árvore de BOM da revisão, multinível, com massa acumulada e quantidade.
- painel de ECO/ECR: estado, revisões incluídas, prontidão (filho não liberado,
  arquivo faltando), quem precisa revisar.
- impacto de uma peça: onde é usada, quais ECO/ECR abertas a tocam.
- diff entre revisões: BOM, arquivos, atributos, material.
- clique na linha da BOM abre a peça no visualizador.

**Aceite:** responder sem abrir o TeamCraft "o que muda na rev. C do PS-001, quem é
afetado e o que trava a liberação".

### Fase 4 — biblioteca, classificação e fornecedor · M

- taxonomia curta de classificação (mecânica, elétrica, comprado, fabricado,
  consumível), aplicada à revisão.
- busca por classe e por semelhança (`find_similar_parts`), duplicata por SHA-256
  (`find_duplicate_files`).
- fornecedor como atributo da revisão (`supplier`, `lead_time_days`) enquanto o
  TeamCraft não tiver tabela; custo já existe (`unit_cost`).
- BOM mostra "comprado/fabricado" e prazo.

**Aceite:** peça nova classificada e encontrada por classe; BOM diz o que é
comprado, de quem e em quanto tempo.

### Fase 5 — variantes e MBOM · G

- variante como atributo de revisão + linha de BOM condicional (ex.: 127 V / 220 V).
  Decidir antes: **item separado por variante** ou **atributo na mesma revisão**.
- MBOM: EBOM + operação (corte, laminação, usinagem) + matéria-prima bruta.
  Começar por uma peça só (núcleo EI: chapa, empilhamento, rebite).

**Aceite:** duas configurações compartilham a mesma definição de peça, e o núcleo
tem roteiro de fabricação com matéria-prima.

### Fase 6 — modelo digital e pendências · G

- visualizador de **conjunto**: ocorrências, explosão, corte, medida, seleção de
  corpo → item de PLM.
- three.js empacotado, não CDN (STYLE §4; hoje a página não abre sem rede).
- painel de pendências no lugar de cronograma: ECO aberta, ECR sem triagem, filho
  não liberado, peça sem arquivo/material/massa, revisão sem ECO.

**Aceite:** o conjunto inteiro navega no `forja-pdm` sem TeamCraft aberto ao lado, e
a tela inicial diz o que falta para liberar.

## 4. Rotas planejadas

Seguem o STYLE §1 (recurso no plural, sem nome de backend).

| rota | devolve | fonte |
|---|---|---|
| `GET /api/materials` | catálogo de materiais | `materials.toml` + base |
| `GET /api/items/<n>/revisions/<rev>/bom` | BOM multinível + massa acumulada | `get_bom` + malha |
| `GET /api/items/<n>/revisions/<rev>/mass` | volume, densidade, massa calculada × declarada | malha + revisão |
| `GET /api/items/<n>/revisions/<rev>/impact` | onde usada, ECO/ECR abertas, filhos travando | `impact_summary` |
| `GET /api/changes?status=` | ECO | `list_changes` |
| `GET /api/changes/<code>` | ECO + revisões + prontidão | `get_change` |
| `GET /api/changes/<code>/diff?from=&to=` | BOM/arquivos/atributos que mudam | `compare_revisions` |
| `GET /api/library?class=&q=` | biblioteca classificada | `search_items` + `find_similar_parts` |
| `POST /api/items/<n>/revisions/<rev>/checkout` | trava a revisão para o app | TeamCraft |
| `POST /api/items/<n>/revisions/<rev>/checkin` | devolve `.sc` + STEP e deriva massa | TeamCraft + kernel |

`/api/items…` continua sendo a rota pública; `/api/teamcraft/items…` fica como
apelido até ninguém mais chamar. As duas rotas `POST` são a única escrita: o resto
é leitura e derivado.

## 5. O que não vamos fazer

- **Hospedar código no PLM (#6).** Código e documento são git; a revisão guarda no
  máximo `git_commit` ou a tag que gerou o STEP.
- **PPM completo (#2).** Gantt, alocação e portfólio são outro produto; a Forja
  mostra **pendência de liberação**.
- **Portal de fornecedor (#8).** Contato e prazo como atributo bastam.
- **Nuvem multi-tenant.** App local, serviço no servidor on-premise, backup local.

## 6. Riscos e decisões abertas

| risco | efeito | saída |
|---|---|---|
| TeamCraft 0.2.7 é binário fechado | não dá para criar tabela; tudo cabe em item, revisão, BOM, arquivo e `attributes` | confirmar se `attributes` é livre antes da fase 2 |
| massa da malha é aproximada (~0,1%) | número divergente do app | massa declarada é a verdade; calculada só confere |
| numeração 2 vs 3 dígitos | STYLE §1 ainda diz 3 dígitos; a ECR-000001 decidiu 2 para o PS-001 | corrigir o STYLE quando a ECR fechar |
| variante sem suporte nativo | duplicar item ou condicionar BOM | decidir na fase 5, com o caso 127/220 V |
| conjunto grande congela a janela do SolidCraft | trabalho em lote travado | lote sempre no `solidcraft-cli`; a janela é para ver e ajustar |
| visualizador depende de CDN | não abre sem rede | empacotar three.js na fase 6 |

## 7. Ordem, em uma linha

**Fase 0** (modelo, feito) → **1** (SolidCraft ↔ PDM) → **2** (catálogo e massa) →
**3** (BOM e ECO na tela) → **4** (biblioteca e fornecedor) → **5** (variante e
MBOM) → **6** (conjunto navegável e pendências).
