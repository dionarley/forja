# PLM — o que a Forja copia do SOLIDWORKS PLM

Referência: página **SOLIDWORKS PLM** (solidworks.com/pt-br/product/solidworks-plm),
lida em 15/11/2026. Ela vende dez recursos, em três pacotes (Standard, Professional,
Premium), sobre a plataforma 3DEXPERIENCE na nuvem.

A Forja não clona o produto: ela tem outro chão — **TeamCraft roda na máquina e é
dono dos dados**, o `forja-pdm` é a janela e a camada de derivados. O que se copia
é o **problema do usuário**, não a tela. Cada recurso passa por três perguntas:

1. O dado já existe no TeamCraft (item, revisão, BOM, arquivo, atributo)?
2. A fronteira do produto permite (STYLE §8: PLM = peça e montagem; documento e
   código = git)?
3. Paga o esforço para 1–5 produtos, ou é recurso de empresa grande?

## 1. Os dez recursos da página

| # | recurso | o que resolve |
|---|---|---|
| 1 | BOM multifuncional | uma definição de produto compartilhada entre disciplinas |
| 2 | Planejamento de projeto conectado ao CAD | tarefas, marcos e dependências ligados ao dado de projeto |
| 3 | Gerenciamento de matérias-primas | matéria-prima e quantidade na BOM |
| 4 | Modelo de produto digital 3D | ver e explorar CAD multi-CAD, inclusive sem CAD |
| 5 | Gerenciamento formal de alterações | ECO em ciclo fechado, com decisão e tarefa para cada área |
| 6 | Integração do código-fonte | código do produto dentro da estrutura/BOM |
| 7 | Classificação IP de bibliotecas | classificar, achar e reutilizar peça |
| 8 | Rede de fornecedores | contato, endereço, rastreabilidade do fornecedor |
| 9 | Itens do fabricante (MBOM) | BOM de fabricação a partir do projeto 3D |
| 10 | Variantes do produto | configurações mestras por regra |

## 2. Onde a Forja está (verificado em 15/11/2026)

| # | estado | onde está hoje |
|---|---|---|
| 1 | **parcial** | TeamCraft tem BOM multinível (`get_bom`) e `bom_health`; o `forja-pdm` não publica BOM nenhuma |
| 2 | **não existe** | — |
| 3 | **parcial** | a revisão já tem `material` e `mass_kg`; não há catálogo de material nem massa calculada da geometria |
| 4 | **parcial** | visualizador STEP/FJPM de **um arquivo**; o conjunto com 22 posições só existe no `.sc` |
| 5 | **parcial** | ECR/ECO, revisão, prontidão de liberação e `impact_summary` completos no TeamCraft; nada disso aparece no `forja-pdm` |
| 6 | **fora de escopo** | decidido: código e documento no git, PLM só guarda o ponteiro (STYLE §8) |
| 7 | **parcial** | `classification` da revisão (`purchased`…) existe, mas sem taxonomia nem busca por classe |
| 8 | **parcial** | só `unit_cost`; fornecedor, prazo e contato não existem |
| 9 | **não existe** | — |
| 10 | **não existe** | — |

O gargalo hoje não é o TeamCraft: é o **modelo** (peça isolada, material, STEP por
peça) e a **janela** (`forja-pdm` só abre um arquivo e só faz GET).

## 3. Fases

Regra: cada fase entrega algo usável sozinha, e nada depois da fase 0 faz sentido
sem ela. Tamanho: **P** ≤ 1 sessão, **M** 2–3, **G** mais que isso.

### Fase 0 — o modelo vira dado (repo do produto)

Pré-requisito de todas as outras. Sem peça nativa, material e STEP por peça, o PLM
não tem o que guardar.

- 14 definições nativas (`PS-001-LM-01…14`) em `cad/solidcraft/parts/`; o conjunto
  mantém 22 posições por link (`component.extract` + `replace_with_link`).
- material por corpo, com densidade; massa por peça.
- STEP por peça e STEP do conjunto.
- BOM do conjunto (part number, descrição, quantidade) igual ao `BOM.csv`.

**Pronto quando:** volume do conjunto idêntico ao de antes (448.433,77 mm³), BOM do
app bate com o `BOM.csv`, e cada peça tem `.sc` + STEP + material + massa.

### Fase 1 — matéria-prima e massa (#3, metade de #1) · P–M

- catálogo de materiais: densidade, custo/kg, classe (metal, polímero, madeira…).
  Base embutida no serviço (a mesma biblioteca do app) + `materials.toml` do produto
  para overrides.
- massa calculada = volume da malha × densidade, **ao lado** da massa declarada na
  revisão; divergência acima de 2% vira aviso.
- `mass_kg` de cada peça preenchido na revisão nova (hoje é `null`).
- na UI: material e massa na ficha da peça e no nó da BOM.

**Pronto quando:** `PS-001` fecha a soma das peças e o conjunto declara a massa
total; nenhuma peça sem `material` ou `mass_kg`.

### Fase 2 — BOM e alterações (#1, #5) · M

- árvore de BOM da revisão, multinível, com massa acumulada e quantidade.
- painel de ECO/ECR: estado, revisões incluídas, prontidão (filho não liberado,
  arquivo faltando), quem precisa revisar.
- impacto de uma peça: onde é usada, quais ECO/ECR abertas a tocam.
- diff entre revisões: BOM, arquivos, atributos, material.
- clique na linha da BOM abre a peça no visualizador.

**Pronto quando:** dá para responder, sem abrir o TeamCraft, "o que muda na rev. C
do PS-001, quem é afetado e o que trava a liberação".

### Fase 3 — biblioteca, classificação e fornecedor (#7, #8) · M

- taxonomia curta de classificação (mecânica, elétrica, comprado, fabricado,
  consumível), aplicada à revisão.
- busca por classe e por semelhança (`find_similar_parts`), duplicata por SHA-256
  (`find_duplicate_files`).
- fornecedor como atributo da revisão (`supplier`, `lead_time_days`) enquanto o
  TeamCraft não tiver tabela; custo já existe (`unit_cost`).
- BOM mostra coluna "comprado/fabricado" e prazo.

**Pronto quando:** uma peça nova é classificada e encontrada por classe, e a BOM
diz o que é comprado, de quem e em quanto tempo.

### Fase 4 — variantes e MBOM (#10, #9) · G

- variante como atributo de revisão + linha de BOM condicional (ex.: 127 V / 220 V).
  Decidir antes: **item separado por variante** ou **atributo na mesma revisão** —
  a primeira é mais simples no TeamCraft, a segunda evita duplicar BOM.
- MBOM: EBOM + operação (corte, laminação, usinagem) + matéria-prima bruta.
  Começar por uma peça só (núcleo EI: chapa, empilhamento, rebite), não pelo produto.

**Pronto quando:** o conjunto tem duas configurações que compartilham a mesma
definição de peça, e o núcleo tem roteiro de fabricação com matéria-prima.

### Fase 5 — modelo digital e pendências (#4, #2) · G

- visualizador de **conjunto**: ocorrências, explosão, corte, medida, seleção de
  corpo → item de PLM.
- three.js empacotado, não CDN (STYLE §4; hoje a página não abre sem rede).
- painel de pendências no lugar de cronograma: ECO aberta, ECR sem triagem, filho
  não liberado, peça sem arquivo/material/massa, revisão sem ECO.

**Pronto quando:** o conjunto inteiro abre e navega no `forja-pdm` sem TeamCraft
aberto ao lado, e a tela inicial diz o que falta para liberar.

## 4. Rotas planejadas

Seguem o STYLE §1 (recurso no plural, sem nome de backend) e são todas de leitura:
o `forja-pdm` não cria item nem libera revisão.

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
| `GET /api/items/<n>/revisions/<rev>/manufacturing` | MBOM e roteiro | nova (fase 4) |

`/api/items…` continua sendo a rota pública; `/api/teamcraft/items…` fica como
apelido até ninguém mais chamar.

## 5. O que não vamos fazer

- **Hospedar código no PLM (#6).** Código e documento são git; a revisão guarda no
  máximo o ponteiro (`git_commit` ou tag) para rastrear qual código gerou o STEP.
- **PPM completo (#2).** Gantt, alocação e portfólio são outro produto. A Forja
  mostra **pendência de liberação**, que é o que trava a entrega.
- **Portal de fornecedor (#8).** Contato e prazo como atributo bastam; fornecedor
  não entra no sistema para responder cotação.
- **Nuvem multi-tenant.** Tudo continua em `127.0.0.1`, com backup local.

## 6. Riscos e decisões abertas

| risco | efeito | saída |
|---|---|---|
| TeamCraft 0.2.7 é binário fechado | não dá para criar tabela; tudo tem que caber em item, revisão, BOM, arquivo e `attributes` | confirmar se `attributes` é livre antes da fase 2; se não for, guardar JSON em arquivo da revisão |
| massa da malha é aproximada (~0,1%) | número divergente do app | massa declarada é a verdade; calculada só confere e avisa |
| numeração 2 vs 3 dígitos | STYLE §1 ainda diz 3 dígitos; a decisão da ECR-000001 foi 2 para o PS-001 | corrigir o STYLE quando a ECR fechar |
| variante sem suporte nativo | duplicar item ou condicionar BOM | decidir na fase 4, com o caso 127/220 V do Brasa |
| visualizador depende de CDN | não abre sem rede | empacotar three.js na fase 5 |
| conjunto com 22 posições | malha de 54 mil triângulos já é o teto confortável | medir antes de prometer conjunto grande |

## 7. Ordem, em uma linha

**Fase 0** (peça nativa + material + massa) → **1** (catálogo e massa) →
**2** (BOM e ECO na tela) → **3** (biblioteca e fornecedor) → **4** (variante e
MBOM) → **5** (conjunto navegável e pendências).
