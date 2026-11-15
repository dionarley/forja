#!/usr/bin/env bash
# Controla o serviço forja-pdm em segundo plano (PID e log em .run/).
#
#   scripts/service.sh start|stop|restart|status [endereço]
#
# O endereço padrão é 127.0.0.1:8090 (FORJA_PDM_ADDR muda). O serviço precisa de
# FORJA_TEAMCRAFT_TOKEN no ambiente ou do credentials.txt da instalação local do
# TeamCraft — nunca coloque o token aqui.
set -euo pipefail

cd "$(dirname "$0")/.."
root=$(pwd)
run="$root/.run"
pidfile="$run/forja-pdm.pid"
logfile="$run/forja-pdm.log"
addr="${2:-${FORJA_PDM_ADDR:-127.0.0.1:8090}}"
bin="$root/target/debug/forja-pdm"

mkdir -p "$run"

running() {
  [ -f "$pidfile" ] && kill -0 "$(cat "$pidfile")" 2>/dev/null
}

start() {
  if running; then
    echo "já rodando (pid $(cat "$pidfile")) em http://$addr"
    return
  fi
  if [ ! -x "$bin" ]; then
    echo "compilando…"
    cargo build -p forja-pdm
  fi
  nohup "$bin" serve "$addr" >>"$logfile" 2>&1 &
  echo $! >"$pidfile"
  sleep 1
  if running; then
    echo "iniciado (pid $(cat "$pidfile")) em http://$addr — log: $logfile"
  else
    echo "falhou; veja $logfile" >&2
    tail -n 20 "$logfile" >&2 || true
    exit 1
  fi
}

stop() {
  if ! running; then
    echo "não está rodando"
    rm -f "$pidfile"
    return
  fi
  local pid
  pid=$(cat "$pidfile")
  kill "$pid" 2>/dev/null || true
  for _ in $(seq 20); do
    kill -0 "$pid" 2>/dev/null || break
    sleep 0.25
  done
  kill -9 "$pid" 2>/dev/null || true
  rm -f "$pidfile"
  echo "parado (pid $pid)"
}

status() {
  if running; then
    echo "rodando (pid $(cat "$pidfile")) em http://$addr"
    curl -s "http://$addr/healthz" | head -c 600 || true
    echo
  else
    echo "parado"
    return 1
  fi
}

case "${1:-status}" in
  start) start ;;
  stop) stop ;;
  restart) stop; start ;;
  status) status ;;
  *) echo "uso: $0 start|stop|restart|status [endereço]" >&2; exit 2 ;;
esac
