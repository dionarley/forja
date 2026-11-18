# vendor

Bibliotecas de terceiro embutidas no `forja-pdm` (o visualizador abre sem
internet). Não edite: troque a versão e a licença juntas.

| arquivo | origem | licença |
|---|---|---|
| `three.module.min.js` | `three@0.170.0/build/three.module.min.js` (jsDelivr) | MIT, ver `LICENSE.three` |
| `OrbitControls.js` | `three@0.170.0/examples/jsm/controls/OrbitControls.js` | MIT, ver `LICENSE.three` |

O `OrbitControls.js` importa `three` pelo nome; o mapa de importação do
`index.html` resolve para `/vendor/three.module.min.js`.
