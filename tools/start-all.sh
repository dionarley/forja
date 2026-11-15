#!/usr/bin/env bash
# Sobe o que a plataforma precisa para trabalhar: TeamCraft e forja-pdm.
#
#   tools/start-all.sh
#
# Idempotente: o que já está no ar fica como está. Não compila — se o binário do
# forja-pdm não existir, `scripts/service.sh start` compila (e o primeiro build
# leva ~25 min por causa do kernel).
set -euo pipefail

cd "$(dirname "$0")/.."
root=$(pwd)

say() { printf '%-12s %s\n' "$1" "$2"; }

# TeamCraft: serviço de usuário do systemd (ele mesmo sobe o banco em podman).
if systemctl --user list-unit-files teamcraft.service >/dev/null 2>&1; then
  if systemctl --user is-active --quiet teamcraft.service; then
    say teamcraft "já ativo (127.0.0.1:8080)"
  else
    systemctl --user start teamcraft.service
    say teamcraft "iniciado (127.0.0.1:8080)"
  fi
else
  say teamcraft "serviço não instalado — veja services/teamcraft/README.md"
fi

# forja-pdm: processo nosso, PID em .run/.
if scripts/service.sh status >/dev/null 2>&1; then
  say forja-pdm "$(scripts/service.sh status | head -1)"
else
  scripts/service.sh start
fi

echo
"$root/tools/status.sh" || true
