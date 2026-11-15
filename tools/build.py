#!/usr/bin/env python3
"""Acompanha o build do forja-pdm: log com carimbo de tempo, progresso e guarda de memória.

    tools/build.sh start [args do cargo]   # build em segundo plano, sempre logado
    tools/build.sh status                  # retrato do build (funciona em build alheio)
    tools/build.sh watch                   # retrato ao vivo, 2 s
    tools/build.sh log [-n 30] [-f]        # saída do cargo, com hora
    tools/build.sh errors                  # só diagnósticos (error/warning)
    tools/build.sh stop                    # mata o build e o que ele gerou
    tools/build.sh history                 # builds anteriores: tempo, pico de RAM
    tools/build.sh timings [--open]        # gantt do cargo (target/cargo-timings)

Por que existe: o primeiro build leva ~25 min e um `rustc` do kernel chega a
6-7 GB. Sem log, não dá para saber se está compilando, linkando, esperando lock
ou prestes a derrubar a máquina. O `start` grava cada linha com o instante em
que chegou, então o tempo de cada crate sai do próprio log.
"""

from __future__ import annotations

import json
import os
import re
import shutil
import signal
import subprocess
import sys
import threading
import time
from datetime import datetime
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
RUN = ROOT / ".run" / "build"
LOG = RUN / "build.log"
PID = RUN / "pid"
META = RUN / "meta"
TOTAL = RUN / "total"
PEAK = RUN / "peak"
START = RUN / "start"
HISTORY = RUN / "history.tsv"
DEFAULT_ARGS = ["-p", "forja-pdm"]
MEM_LIMIT_MB = int(os.environ.get("FORJA_BUILD_MEM_MB", "9000"))

COMPILING = re.compile(r"^\s*Compiling (\S+)(?: v(\S+))?")
LOCK = "Blocking waiting for file lock"
DIAG = re.compile(r"^(error|warning)(\[[^\]]+\])?:")

_c = sys.stdout.isatty()


def paint(code: str, text: str) -> str:
    return f"\033[{code}m{text}\033[0m" if _c else text


def dim(t: str) -> str:
    return paint("2", t)


def red(t: str) -> str:
    return paint("31", t)


def green(t: str) -> str:
    return paint("32", t)


def yellow(t: str) -> str:
    return paint("33", t)


def duration(seconds: float) -> str:
    seconds = int(max(0, seconds))
    if seconds < 60:
        return f"{seconds} s"
    if seconds < 3600:
        return f"{seconds // 60}m{seconds % 60:02d}s"
    return f"{seconds // 3600}h{seconds % 3600 // 60:02d}m"


def clock(seconds: float) -> str:
    seconds = int(max(0, seconds))
    return f"{seconds // 60:02d}:{seconds % 60:02d}" if seconds < 3600 else duration(seconds)


def gb(kb: int) -> str:
    return f"{kb / 1024 / 1024:.1f} GB".replace(".", ",")


def gb_of_mb(mb: int) -> str:
    return f"{mb / 1024:.1f} GB".replace(".", ",")


def gb_of_bytes(nbytes: int) -> str:
    return f"{nbytes / 1024 ** 3:.1f} GB".replace(".", ",")


def bar(frac: float, width: int = 30) -> str:
    frac = min(1.0, max(0.0, frac))
    full = int(frac * width)
    return "[" + "=" * full + (">" if full < width else "") + " " * (width - full - 1) + "]"


# ---------------------------------------------------------------- processos


class Proc:
    __slots__ = ("pid", "ppid", "pgid", "name", "rss_kb", "argv", "cwd")

    def __init__(self, pid: int):
        self.pid = pid
        try:
            stat = Path(f"/proc/{pid}/stat").read_text()
            rest = stat[stat.rfind(")") + 2 :].split()
            self.ppid, self.pgid = int(rest[1]), int(rest[2])
            self.name = stat[stat.find("(") + 1 : stat.rfind(")")]
            pages = int(Path(f"/proc/{pid}/statm").read_text().split()[1])
            self.rss_kb = pages * os.sysconf("SC_PAGE_SIZE") // 1024
            self.argv = [
                a.decode("utf-8", "replace")
                for a in Path(f"/proc/{pid}/cmdline").read_bytes().split(b"\0")
                if a
            ]
            try:
                self.cwd = os.readlink(f"/proc/{pid}/cwd")
            except OSError:
                self.cwd = ""
        except (OSError, ValueError, IndexError):
            raise FileNotFoundError(pid)


def processes() -> list[Proc]:
    out = []
    for entry in Path("/proc").iterdir():
        if entry.name.isdigit():
            try:
                out.append(Proc(int(entry.name)))
            except (FileNotFoundError, ProcessLookupError):
                pass
    return out


def tool_name(proc: Proc) -> str:
    """Nome real da ferramenta (o wrapper do nix troca o comm por .cargo-wrapped)."""
    return os.path.basename(proc.argv[0]) if proc.argv else proc.name


def is_tool(proc: Proc) -> bool:
    return tool_name(proc) in {"rustc", "cargo", "cc1", "cc1plus", "ld", "collect2"}


def in_root(proc: Proc) -> bool:
    """O processo trabalha neste repo (pela linha de comando ou pelo diretório)."""
    root = str(ROOT)
    return root in " ".join(proc.argv) or proc.cwd.startswith(root + "/") or proc.cwd == root


def crate_name(argv: list[str]) -> str | None:
    for i, arg in enumerate(argv):
        if arg == "--crate-name" and i + 1 < len(argv):
            return argv[i + 1]
    return None


def toolchain_mem(procs: list[Proc]) -> tuple[int, list[Proc]]:
    """Soma o RSS de rustc/cc1/ld e devolve os processos que importam."""
    mine = [p for p in procs if is_tool(p)]
    return sum(p.rss_kb for p in mine), mine


def mem_available_mb() -> int:
    for line in Path("/proc/meminfo").read_text().splitlines():
        if line.startswith("MemAvailable:"):
            return int(line.split()[1]) // 1024
    return 0


# --------------------------------------------------------------------- log


class Event:
    __slots__ = ("t", "text")

    def __init__(self, t: float, text: str):
        self.t, self.text = t, text


def read_log() -> list[Event]:
    events: list[Event] = []
    if not LOG.exists():
        return events
    with LOG.open(encoding="utf-8", errors="replace") as fh:
        for line in fh:
            stamp, sep, text = line.partition("\t")
            if not sep:
                continue
            try:
                events.append(Event(float(stamp), text.rstrip("\n")))
            except ValueError:
                continue
    return events


def summarize(events: list[Event]) -> dict:
    """Unidades feitas, crate atual, durações e diagnósticos, tudo do log."""
    done = 0
    starts: dict[str, float] = {}
    durations: list[tuple[str, float]] = []
    warnings = errors = 0
    locked = 0.0
    finished = None
    for ev in events:
        text = ev.text
        m = COMPILING.match(text)
        if m:
            starts[m.group(1)] = ev.t
            continue
        if text.startswith("{"):
            try:
                msg = json.loads(text)
            except ValueError:
                msg = None
            if isinstance(msg, dict):
                reason = msg.get("reason")
                if reason == "compiler-artifact":
                    done += 1
                    name = (msg.get("target") or {}).get("name")
                    if name and name in starts:
                        durations.append((name, ev.t - starts.pop(name)))
                elif reason == "build-script-executed":
                    done += 1
                elif reason == "build-finished":
                    finished = msg.get("success")
                continue
        if LOCK in text:
            locked = ev.t
        if DIAG.match(text):
            if text.startswith("warning"):
                warnings += 1
            else:
                errors += 1
            if text.startswith("error"):
                # o erro vem com o bloco inteiro; conta uma vez por cabeçalho
                starts.clear()
    active = list(starts.items())
    return {
        "done": done,
        "active": active,
        "durations": durations,
        "warnings": warnings,
        "errors": errors,
        "locked": locked,
        "finished": finished,
        "first": events[0].t if events else None,
        "last": events[-1].t if events else None,
    }


def read_total() -> int:
    try:
        return int(TOTAL.read_text().strip())
    except (OSError, ValueError):
        return 0


def read_peak() -> int:
    try:
        return int(PEAK.read_text().strip())
    except (OSError, ValueError):
        return 0


def cargo_args() -> str:
    try:
        return META.read_text().strip()
    except OSError:
        return " ".join(DEFAULT_ARGS)


# ---------------------------------------------------------------- relatório


def read_int(path: Path) -> int | None:
    try:
        return int(path.read_text().strip())
    except (OSError, ValueError):
        return None


def group_alive(pgid: int) -> bool:
    try:
        os.killpg(pgid, 0)
        return True
    except ProcessLookupError:
        return False


def read_pid() -> tuple[int | None, int | None]:
    """(grupo, pid) do build em andamento — o grupo manda no stop."""
    try:
        lines = PID.read_text().split()
    except OSError:
        return None, None
    values = []
    for item in lines[:2]:
        try:
            values.append(int(item))
        except ValueError:
            values.append(None)
    while len(values) < 2:
        values.append(None)
    return values[0], values[1]


def artifact_stats() -> tuple[int, int]:
    """(rlibs, bytes) do target/debug/deps — serve de régua para build sem log."""
    deps = ROOT / "target" / "debug" / "deps"
    rlibs = 0
    total = 0
    if not deps.is_dir():
        return 0, 0
    for entry in os.scandir(deps):
        try:
            if entry.name.endswith((".rlib", ".rmeta")):
                rlibs += 1
            total += entry.stat().st_size
        except OSError:
            continue
    return rlibs, total


def idle_lines() -> list[str]:
    out = ["", "  nenhum build em andamento."]
    if HISTORY.exists():
        last = HISTORY.read_text().splitlines()[-1:]
        if last:
            parts = last[0].split("\t")
            if len(parts) >= 6:
                out.append(f"  último: {parts[0]} · {parts[1]} · {parts[2]} unidades · {parts[3]}")
    binary = ROOT / "target" / "debug" / "forja-pdm"
    if binary.exists():
        when = datetime.fromtimestamp(binary.stat().st_mtime)
        out.append(f"  binário target/debug/forja-pdm de {when:%d/%m %H:%M}")
    out.append(dim("  comece com: tools/build.sh start"))
    return out


def foreign_lines(procs: list[Proc], tool_kb: int, free_mb: int, guard: str) -> list[str]:
    """Build iniciado fora da ferramenta: não há log, só dá para ver os processos."""
    out = [dim("  iniciado fora da ferramenta — saída num pipe, sem log")]
    for p in procs:
        if tool_name(p) == "cargo" and in_root(p):
            out.append(f"  cargo   pid {p.pid} · há {duration(proc_age(p.pid))}")
    for name, kb, age in active_crates(procs):
        out.append(f"  agora   {name} · {gb(kb)} · rustc há {duration(age)}")
    out.append(
        f"  memória {gb(tool_kb)} em rustc/cargo · {gb_of_mb(free_mb)} livre · {guard}"
    )
    rlibs, size = artifact_stats()
    if rlibs:
        out.append(f"  artefatos {rlibs} .rlib/.rmeta · {gb_of_bytes(size)} em deps")
    out.append(dim("  da próxima vez: tools/build.sh start (log com hora, progresso e pico)"))
    return out


def status_lines() -> list[str]:
    events = read_log()
    info = summarize(events)
    procs = processes()
    tool_kb, _ = toolchain_mem(procs)
    free_mb = mem_available_mb()
    guard = f"guarda {MEM_LIMIT_MB} MB"
    pgid, pid = read_pid()
    alive = pgid is not None and group_alive(pgid)
    ours = alive and META.exists()

    out = [f"{paint('1', 'Forja')} · build de {cargo_args()}"]

    if not ours:
        foreign = [p for p in procs if tool_name(p) in {"cargo", "rustc"} and in_root(p)]
        return out + (foreign_lines(procs, tool_kb, free_mb, guard) if foreign else idle_lines())

    now = time.time()
    start = info["first"] or read_int(START) or now
    elapsed = now - start
    stale = now - (info["last"] or start)
    total = read_total()
    done = info["done"]

    head = f"  começou {datetime.fromtimestamp(start):%H:%M} · {duration(elapsed)} de build"
    head += f" · pid {pid}" if alive else dim(" · processo terminou")
    out.append(head)
    out.append("")

    if total:
        frac = done / total
        eta = (elapsed / done) * (total - done) if done else 0
        out.append(
            f"  {bar(frac)} {done}/{total} unidades · {frac * 100:.0f}%"
            f" · restam ~{duration(eta)}"
        )
    elif done:
        out.append(f"  {done} unidades compiladas" + dim(" (total ainda desconhecido)"))
    else:
        out.append(dim("  nenhuma unidade compilada ainda"))
    out.append("")

    if info["active"]:
        for name, since in info["active"][:2]:
            rss = crate_rss(procs, name)
            line = f"  agora    {name} há {clock(now - since)}"
            out.append(line + (f" · {gb(rss)}" if rss else ""))
    elif alive and done:
        out.append(dim("  agora    (entre unidades — linkando ou gravando artefatos)"))

    out.append(
        f"  memória  {gb(tool_kb)} em rustc/cargo · {gb_of_mb(free_mb)} livre · {guard}"
        + (f" · pico {gb(read_peak() * 1024)}" if read_peak() else "")
    )

    recent = info["durations"][-1:]
    if recent:
        name, seconds = recent[0]
        out.append(f"  último   {name} em {duration(seconds)}")

    if info["errors"] or info["warnings"]:
        bits = []
        if info["errors"]:
            bits.append(red(f"{info['errors']} erro(s)"))
        if info["warnings"]:
            bits.append(yellow(f"{info['warnings']} aviso(s)"))
        out.append("  saída    " + " · ".join(bits) + dim(" — tools/build.sh errors"))
    else:
        out.append("  saída    " + dim("0 erros, 0 avisos"))

    if info["locked"] and now - info["locked"] < 600:
        out.append(
            yellow("  espera   lock do target/ — outro cargo está compilando")
            + dim(" (tools/build.sh status mostra o outro)")
        )
    elif alive and not info["active"] and stale > 120:
        out.append(
            yellow(f"  parado   sem linha nova há {duration(stale)}")
            + dim(" — link (ld) é silencioso; acima de 5 min veja tools/build.sh log")
        )
    if alive:
        out.append(dim(f"  log      {LOG.relative_to(ROOT)} (última linha há {duration(stale)})"))
    if info["finished"] is not None:
        out.append("")
        out.append(green("  build terminado") if info["finished"] else red("  build falhou"))
    return out


def boot_time() -> float:
    for line in Path("/proc/stat").read_text().splitlines():
        if line.startswith("btime"):
            return float(line.split()[1])
    return time.time()


def proc_age(pid: int) -> float:
    """Segundos desde que o processo nasceu (starttime do /proc, em ticks)."""
    try:
        stat = Path(f"/proc/{pid}/stat").read_text()
        rest = stat[stat.rfind(")") + 2 :].split()
        started = int(rest[19]) / os.sysconf("SC_CLK_TCK")
        return max(0.0, time.time() - (boot_time() + started))
    except (OSError, ValueError, IndexError):
        return 0.0


def active_crates(procs: list[Proc]) -> list[tuple[str, int, float]]:
    """(crate, rss_kb, instante de início aproximado) dos rustc vivos do repo."""
    out = []
    for p in procs:
        if tool_name(p) != "rustc":
            continue
        name = crate_name(p.argv)
        if name and in_root(p):
            out.append((name, p.rss_kb, proc_age(p.pid)))
    return out


def crate_rss(procs: list[Proc], name: str) -> int:
    for p in procs:
        if tool_name(p) == "rustc" and crate_name(p.argv) == name:
            return p.rss_kb
    return 0


# ------------------------------------------------------------------ comandos


def cmd_status(_args: list[str]) -> int:
    print("\n".join(status_lines()))
    return 0


def cmd_watch(args: list[str]) -> int:
    interval = 2.0
    if args:
        try:
            interval = float(args[0])
        except ValueError:
            pass
    if not _c:
        print("\n".join(status_lines()))
        return 0
    try:
        while True:
            body = "\n".join(status_lines())
            sys.stdout.write("\033[H\033[2J" + body + "\n")
            sys.stdout.write(dim(f"\n  (Ctrl+C sai · atualiza a cada {interval:g} s)"))
            sys.stdout.flush()
            time.sleep(interval)
    except KeyboardInterrupt:
        print()
    return 0


def cmd_log(args: list[str]) -> int:
    tail = 30
    follow = False
    for i, arg in enumerate(args):
        if arg in {"-n", "--lines"} and i + 1 < len(args):
            tail = int(args[i + 1])
        elif arg in {"-f", "--follow"}:
            follow = True
    if not LOG.exists():
        print("sem log — o build atual não foi iniciado por tools/build.sh", file=sys.stderr)
        return 1
    with LOG.open(encoding="utf-8", errors="replace") as fh:
        lines = fh.readlines()
    for line in lines[-tail:]:
        stamp, _, text = line.partition("\t")
        try:
            when = datetime.fromtimestamp(float(stamp)).strftime("%H:%M:%S")
        except ValueError:
            when, text = "", line
        print(f"{dim(when)} {text.rstrip()}")
    if follow:
        pos = LOG.stat().st_size
        try:
            while True:
                time.sleep(0.5)
                size = LOG.stat().st_size
                if size > pos:
                    with LOG.open(encoding="utf-8", errors="replace") as fh:
                        fh.seek(pos)
                        chunk = fh.read()
                    pos = size
                    for line in chunk.splitlines():
                        stamp, _, text = line.partition("\t")
                        try:
                            when = datetime.fromtimestamp(float(stamp)).strftime("%H:%M:%S")
                        except ValueError:
                            when, text = "", line
                        print(f"{dim(when)} {text}")
                    sys.stdout.flush()
        except KeyboardInterrupt:
            pass
    return 0


def cmd_errors(_args: list[str]) -> int:
    events = read_log()
    hits = [e for e in events if DIAG.match(e.text)]
    if not hits:
        print(green("nenhum erro ou aviso no log"))
        return 0
    for ev in hits:
        when = datetime.fromtimestamp(ev.t).strftime("%H:%M:%S")
        line = f"{dim(when)} {ev.text}"
        print(red(line) if ev.text.startswith("error") else yellow(line))
    print(dim(f"\n{len(hits)} diagnóstico(s) — cargo build mostra o contexto completo"))
    return 0


def running() -> tuple[int, int] | None:
    """(pgid, pid) do build em andamento, se houver."""
    pgid, pid = read_pid()
    if pgid is None or not group_alive(pgid):
        return None
    return pgid, pid or pgid


def cmd_start(args: list[str]) -> int:
    current = running()
    if current:
        print(f"já há um build rodando (pid {current[1]}) — tools/build.sh status")
        return 1
    RUN.mkdir(parents=True, exist_ok=True)
    cargo = args or DEFAULT_ARGS
    META.write_text(" ".join(cargo) + "\n")
    LOG.write_text("")
    PEAK.write_text("")
    START.write_text(f"{time.time():.3f}\n")
    proc = subprocess.Popen(
        [sys.executable, str(Path(__file__).resolve()), "__run", *cargo],
        cwd=ROOT,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        start_new_session=True,
    )
    for _ in range(40):
        if PID.exists() and PID.read_text().strip():
            break
        time.sleep(0.05)
    print(f"build iniciado (pid {proc.pid}) · log {LOG.relative_to(ROOT)}")
    print("acompanhe com: tools/build.sh watch")
    return 0


def cmd_stop(_args: list[str]) -> int:
    current = running()
    if not current:
        print("nenhum build em andamento")
        return 0
    pgid, pid = current
    try:
        os.killpg(pgid, signal.SIGTERM)
    except ProcessLookupError:
        pass
    for _ in range(50):
        time.sleep(0.1)
        if not group_alive(pgid):
            break
    if group_alive(pgid):
        try:
            os.killpg(pgid, signal.SIGKILL)
        except ProcessLookupError:
            pass
    print(f"build {pid} interrompido (grupo {pgid})")
    return 0


def cmd_history(_args: list[str]) -> int:
    if not HISTORY.exists() or not HISTORY.read_text().strip():
        print("sem histórico ainda")
        return 0
    print(f"{'quando':16} {'resultado':8} {'unidades':>8} {'tempo':>8} {'pico':>8}  args")
    for line in HISTORY.read_text().splitlines():
        parts = line.split("\t")
        if len(parts) < 6:
            continue
        when, result, total, seconds, peak, args = parts[:6]
        colour = green if result == "ok" else (yellow if result == "interrompido" else red)
        print(
            f"{when:16} {colour(f'{result:8}')} {total:>8} {seconds:>8} "
            f"{peak + ' MB':>8}  {args}"
        )
    return 0


def cmd_timings(args: list[str]) -> int:
    folder = ROOT / "target" / "cargo-timings"
    reports = sorted(folder.glob("cargo-timing-*.html")) if folder.exists() else []
    if not reports:
        print("nenhum relatório ainda — o build grava com --timings ao terminar")
        return 1
    newest = reports[-1]
    print(newest)
    if "--open" in args:
        opener = shutil.which("xdg-open")
        if opener:
            subprocess.Popen([opener, str(newest)])
    return 0


# --------------------------------------------------------------- supervisor


def supervise(args: list[str]) -> int:
    RUN.mkdir(parents=True, exist_ok=True)
    me = os.getpid()
    pgid = os.getpgid(0)
    PID.write_text(f"{pgid}\n{me}\n")
    PEAK.write_text("0\n")
    cmd = ["cargo", "build", *args, "--message-format=json-render-diagnostics", "--timings"]
    started = time.time()
    START.write_text(f"{started:.3f}\n")
    peak = 0
    killed = False

    proc = subprocess.Popen(
        cmd, cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, bufsize=1
    )

    stopped = {"flag": False}

    def on_stop(_signum, _frame):
        stopped["flag"] = True
        try:
            proc.kill()
        except OSError:
            pass

    signal.signal(signal.SIGTERM, on_stop)
    signal.signal(signal.SIGINT, on_stop)

    def pump() -> None:
        assert proc.stdout is not None
        with LOG.open("a", encoding="utf-8") as log:
            for line in proc.stdout:
                log.write(f"{time.time():.3f}\t{line}")
                log.flush()

    thread = threading.Thread(target=pump, daemon=True)
    thread.start()

    while proc.poll() is None:
        time.sleep(2)
        try:
            rss_kb, _ = toolchain_mem([p for p in processes() if p.pgid == pgid])
        except OSError:
            rss_kb = 0
        mb = rss_kb // 1024
        if mb > peak:
            peak = mb
            PEAK.write_text(f"{mb}\n")
        if mb > MEM_LIMIT_MB:
            killed = True
            with LOG.open("a", encoding="utf-8") as log:
                log.write(f"{time.time():.3f}\tGUARD: {mb} MB acima de {MEM_LIMIT_MB} MB\n")
            try:
                os.killpg(pgid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            break

    thread.join(timeout=5)
    code = proc.poll()
    elapsed = time.time() - started
    info = summarize(read_log())
    total = info["done"]
    if total:
        TOTAL.write_text(f"{total}\n")
    if killed:
        result = "guarda"
    elif stopped["flag"]:
        result = "interrompido"
    else:
        result = "ok" if code == 0 else "erro"
    with HISTORY.open("a", encoding="utf-8") as fh:
        fh.write(
            f"{datetime.now():%Y-%m-%d %H:%M}\t{result}\t{total}\t{duration(elapsed)}"
            f"\t{peak}\t{' '.join(args)}\n"
        )
    try:
        PID.unlink()
    except OSError:
        pass
    return 0 if code == 0 and not killed else 1


COMMANDS = {
    "start": cmd_start,
    "status": cmd_status,
    "watch": cmd_watch,
    "log": cmd_log,
    "errors": cmd_errors,
    "stop": cmd_stop,
    "history": cmd_history,
    "timings": cmd_timings,
    "__run": supervise,
}


def main(argv: list[str]) -> int:
    if not argv or argv[0] in {"-h", "--help", "help"}:
        print(__doc__.strip())
        return 0
    cmd = COMMANDS.get(argv[0])
    if not cmd:
        print(f"comando desconhecido: {argv[0]} (veja tools/build.sh --help)", file=sys.stderr)
        return 2
    return cmd(argv[1:])


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
