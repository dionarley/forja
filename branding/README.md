# Branding

A **Forja** é plataforma, não produto: ela não tem marca pública própria. O que
existe aqui é o vocabulário visual que os serviços da plataforma (visualizador,
docs, `forja-pdm`) usam para não parecerem cinco ferramentas diferentes.

## De onde vêm os tokens

Da marca Tessera, repo privado `dionarley/tessera`:

| token | hex | uso na plataforma |
|---|---|---|
| `graphite` | `#1B1E22` | fundo do visualizador, cabeçalho |
| `paper` | `#F5F2EC` | fundo de documento e de página |
| `steel` | `#2F6F8F` | links, seleção, série principal |
| `copper` | `#B4683A` | alerta, destaque de medição |
| `signal` | `#4C9A6A` | estado ok / liberado |
| `muted` | `#8A8F94` | texto secundário, réguas |

Copiar os tokens para um serviço é `tokens.css` do repo de marca; **não** redefina
hex novo aqui. Se falta um tom, ele entra na marca primeiro.

## O que não entra

- **Marca de produto.** Brasa (e qualquer produto futuro) tem identidade própria,
  em repo próprio. Nada de logo de produto em `crates/` ou no visualizador.
- **O mosaico da Tessera como logo de ferramenta.** A marca assina: rodapé,
  laudo, proposta.
- **Fonte de CDN.** Plex Sans/Mono são OFL; se precisar, empacote.

## Onde os tokens são usados hoje

| peça | arquivo | estado |
|---|---|---|
| visualizador de malha | `crates/forja-pdm/web/index.html` | tokens em CSS inline |
| docs do produto | `diy_hot_gun/site` | tema do Docusaurus (marca do produto) |
| prancha técnica | `diy_hot_gun/tools/plm/template.typ` | tipografia e cotas |

O `gabarito` distribui uma cópia destes tokens para cada produto novo; o produto
sobrescreve com a própria paleta.
