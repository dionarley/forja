#!/usr/bin/env bash
# Materializa os submodules que o serviço precisa e, sob demanda, os demais.
#
#   scripts/sync.sh            # solidcore + open-pdm (path deps do forja-pdm)
#   scripts/sync.sh --all      # todos os 8 forks
#   scripts/sync.sh --status   # só mostra o que está pinado e o que existe
#
# O clone é raso e na branch `forja`. O solidcore perde os goldens
# (`fixtures/`, `crates/goldens/expected/`), que só servem para rodar a suíte do
# próprio kernel e pesam mais que o código.
set -euo pipefail

cd "$(dirname "$0")/.."

NEEDED=(apps/solidcore apps/open-pdm)
OPTIONAL=(apps/solidcraft apps/cadcraft apps/vectorcraft apps/gridcraft
          apps/cad-viewer apps/_gpl/opencadstudio)

materialize() {
  local path=$1
  if [ -e "$path/.git" ]; then
    echo "$path: já presente ($(git -C "$path" rev-parse --short HEAD))"
    return
  fi
  echo "$path: clonando"
  git submodule update --init --depth 1 "$path"
  if [ "$path" = apps/solidcore ]; then
    git -C "$path" sparse-checkout init --no-cone
    git -C "$path" sparse-checkout set '/*' '!/fixtures/' '!/crates/goldens/expected/'
    echo "apps/solidcore: goldens fora do checkout"
  fi
}

status() {
  printf '%-28s %-10s %s\n' submodule pinado materializado
  while read -r mode sha _stage path; do
    [ "$mode" = 160000 ] || continue
    local state=nao
    if [ -e "$path/.git" ]; then
      state=$(git -C "$path" rev-parse --short HEAD)
    fi
    printf '%-28s %-10s %s\n' "$path" "${sha:0:8}" "$state"
  done < <(git ls-files -s | grep '^160000')
}

case "${1:-}" in
  --status) status; exit 0 ;;
  --all) for p in "${NEEDED[@]}" "${OPTIONAL[@]}"; do materialize "$p"; done ;;
  "")    for p in "${NEEDED[@]}"; do materialize "$p"; done ;;
  *) echo "uso: $0 [--all|--status]" >&2; exit 2 ;;
esac

echo
echo "pronto. build: cargo build (primeiro leva ~25 min, o kernel compila inteiro)"
