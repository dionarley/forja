#!/usr/bin/env bash
# Build do forja-pdm com acompanhamento: log com hora, progresso e guarda de RAM.
#
#   tools/build.sh start [args do cargo]   # segundo plano; padrão: -p forja-pdm
#   tools/build.sh status                  # retrato agora (também lê build alheio)
#   tools/build.sh watch [segundos]        # retrato ao vivo (padrão 2 s)
#   tools/build.sh log [-n 30] [-f]        # saída do cargo, com hora
#   tools/build.sh errors                  # só error/warning
#   tools/build.sh stop                    # interrompe o build inteiro
#   tools/build.sh history                 # builds anteriores: tempo e pico de RAM
#   tools/build.sh timings [--open]        # gantt do cargo (target/cargo-timings)
#
# Estado em .run/build/ (ignorado pelo git): build.log, pid, total, peak,
# history.tsv. FORJA_BUILD_MEM_MB muda a guarda (padrão 9000 MB).
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
command -v python3 >/dev/null || { echo "python3 é necessário para o log do build" >&2; exit 1; }
exec python3 "$here/build.py" "$@"
