# Branding

A **Forja** é plataforma, não produto: a marca aqui é a da bancada, derivada da
marca Tessera e usada pelos serviços (`forja-pdm`, visualizador, docs) para não
parecerem cinco ferramentas soltas.

## Símbolo

Bloco de aço com a **boca do forno** em cobre: o lugar onde a peça ganha forma.
Mesma gramática das outras marcas do ecossistema — quadrado, aço, um único
elemento de cobre — sem repetir a peça de nenhuma delas.

| arquivo | uso |
|---|---|
| `logo/mark.svg` | avatar, ícone de app, cabeçalho escuro |
| `logo/favicon.svg` | favicon 32 px |
| `logo/wordmark.svg` | rodapé, assinatura de e-mail |
| `logo/lockup.svg` | cabeçalho de documento e README |

Todos são **contornos** (texto convertido em caminho): não dependem de IBM Plex
instalada e não mudam de largura entre navegador, PDF e Inkscape.

Uso: área de respiro de meia altura do símbolo; mínimo 16 px (símbolo) e 120 px
(lockup); fundo papel ou grafite, nunca foto sem véu.

## De onde vêm os tokens

Da marca Tessera, repo privado `dionarley/tessera`:

| token | hex | uso na plataforma |
|---|---|---|
| `graphite` | `#1B1E22` | fundo do visualizador, cabeçalho |
| `paper` | `#F5F2EC` | fundo de documento e de página |
| `steel` | `#2F6F8F` | links, seleção, série principal, corpo do símbolo |
| `copper` | `#B4683A` | alerta, destaque de medição, boca do forno |
| `signal` | `#4C9A6A` | estado ok / liberado |
| `muted` | `#8A8F94` | texto secundário, réguas |

Copiar os tokens é o `tokens.css` do repo de marca; **não** redefina hex novo
aqui. Se falta um tom, ele entra na marca primeiro.

## O que não entra

- **Marca de produto.** Brasa (e qualquer produto futuro) tem identidade própria,
  em repo próprio. Nada de logo de produto em `crates/` ou no visualizador.
- **O mosaico da Tessera como logo de ferramenta.** A marca assina: rodapé,
  laudo, proposta. A Forja tem o próprio símbolo.
- **Fonte de CDN.** Plex Sans/Mono são OFL; se precisar, empacote.

## Onde os tokens são usados hoje

| peça | arquivo | estado |
|---|---|---|
| visualizador de malha | `crates/forja-pdm/web/index.html` | tokens em CSS inline |
| docs do produto | `diy_hot_gun/site` | tema do Docusaurus (marca do produto) |
| prancha técnica | `diy_hot_gun/tools/plm/template.typ` | tipografia e cotas |

O `gabarito` distribui uma cópia destes tokens para cada produto novo; o produto
sobrescreve com a própria paleta.
