#!/usr/bin/env bash
# Mostra o estado de cada porta da plataforma, em uma linha por serviço.
#
#   tools/status.sh
#
# Só leitura: não sobe, não instala, não derruba nada.
set -uo pipefail

cd "$(dirname "$0")/.." || exit 2

# porta, nome, onde mora
SERVICES=(
  "8080 teamcraft   services/teamcraft"
  "8081 docs        brasa/site (Docusaurus)"
  "8088 gridcraft   brasa/tools/gridcraft"
  "8089 viewer      brasa/tools/cad-viewer"
  "8090 forja-pdm   crates/forja-pdm"
)

code() {
  curl -s -o /dev/null --max-time 2 -w '%{http_code}' "http://127.0.0.1:$1/" 2>/dev/null || true
}

printf '%-6s %-12s %-6s %s\n' porta serviço http onde
fail=0
for line in "${SERVICES[@]}"; do
  read -r port name where <<<"$line"
  state=$(code "$port")
  if [ -z "$state" ] || [ "$state" = 000 ]; then
    state=fora
    fail=1
  fi
  printf '%-6s %-12s %-6s %s\n' "$port" "$name" "$state" "$where"
done

exit "$fail"
