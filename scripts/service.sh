#!/usr/bin/env bash
# Controla o serviço forja-pdm em segundo plano (PID e log em .run/).
#
#   scripts/service.sh start|stop|restart|status [endereço]
#
# Com a unit de usuário instalada (services/forja-pdm.service), o comando delega
# ao systemd: `systemctl --user start|stop|status forja-pdm`. Sem a unit, usa o
# PID e o log em .run/ como antes.
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

unit_installed() {
  systemctl --user cat forja-pdm.service >/dev/null 2>&1
}

running() {
  if unit_installed; then
    systemctl --user is-active --quiet forja-pdm.service
  else
    [ -f "$pidfile" ] && kill -0 "$(cat "$pidfile")" 2>/dev/null
  fi
}

start() {
  if unit_installed; then
    if systemctl --user is-active --quiet forja-pdm.service; then
      echo "já rodando (systemd) em http://$addr"
      return
    fi
    systemctl --user start forja-pdm.service
    sleep 1
    if systemctl --user is-active --quiet forja-pdm.service; then
      echo "iniciado (systemd) em http://$addr — log: journalctl --user -u forja-pdm"
    else
      echo "falhou; veja journalctl --user -u forja-pdm" >&2
      systemctl --user status forja-pdm.service --no-pager >&2 || true
      exit 1
    fi
    return
  fi
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
  if unit_installed; then
    systemctl --user stop forja-pdm.service
    echo "parado (systemd)"
    return
  fi
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
  if unit_installed; then
    if systemctl --user is-active --quiet forja-pdm.service; then
      echo "rodando (systemd, pid $(systemctl --user show -p MainPID --value forja-pdm.service)) em http://$addr"
      curl -s "http://$addr/healthz" | head -c 600 || true
      echo
    else
      echo "parado (systemd)"
      return 1
    fi
    return
  fi
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
