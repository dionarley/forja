//! Abrir a peça nativa no app SolidCraft.
//!
//! O `.sc` vive no vault do TeamCraft, que o sandbox do flatpak não enxerga.
//! O serviço materializa o arquivo em `~/Documentos/forja-cad` (o `xdg-documents`
//! está liberado para o app) e pede a abertura pelo protocolo de controle do
//! SolidCraft — JSON por linha com o token de `data/solidcraft/control/`.
//!
//! Sem app rodando, o plano B é `flatpak run io.tangencylabs.solidcraft <arquivo>`;
//! se nem isso der, o caminho volta na resposta para abrir à mão.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};

use crate::server::App;

/// Resultado da tentativa de falar com o app.
enum Control {
    Opened,
    Refused(String),
    Unreachable(String),
}

/// `GET /api/cad?code=&rev=` — leva a peça ao SolidCraft e diz onde ela ficou.
pub fn open(app: &App, code: &str, revision: &str) -> Result<Value> {
    let files = app.client.files(code, revision)?;
    let found = files
        .iter()
        .find(|f| f.filename.to_ascii_lowercase().ends_with(".sc"))
        .ok_or_else(|| {
            anyhow!(
                "{code}@{revision} não tem .sc (tem: {})",
                files
                    .iter()
                    .map(|f| f.path())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;
    let bytes = app.client.object(&found.sha256)?;

    let dir = output_dir()?;
    std::fs::create_dir_all(&dir).with_context(|| format!("criar {}", dir.display()))?;
    let path = dir.join(format!("{code}-{revision}.sc"));
    let same = std::fs::read(&path)
        .map(|old| old == bytes)
        .unwrap_or(false);
    if !same {
        let temp = dir.join(format!(".{code}-{revision}.sc.tmp"));
        std::fs::write(&temp, &bytes).with_context(|| format!("gravar {}", temp.display()))?;
        std::fs::rename(&temp, &path).with_context(|| format!("renomear {}", path.display()))?;
    }
    let path = std::fs::canonicalize(&path).unwrap_or(path);

    let (opened, mode, error) = match control(&path) {
        Control::Opened => (true, "control", None),
        Control::Refused(error) => (false, "control", Some(error)),
        Control::Unreachable(reason) => match launch(&path) {
            Ok(()) => (true, "launch", None),
            Err(error) => (false, "none", Some(format!("{reason}; {error}"))),
        },
    };
    Ok(json!({
        "code": code,
        "revision": revision,
        "path": path,
        "opened": opened,
        "mode": mode,
        "error": error,
    }))
}

/// O diretório que o app enxerga. `FORJA_CAD_DIR` manda, para quem quiser outro.
fn output_dir() -> Result<PathBuf> {
    if let Ok(dir) = std::env::var("FORJA_CAD_DIR") {
        if !dir.trim().is_empty() {
            return Ok(PathBuf::from(dir));
        }
    }
    let home = std::env::var("HOME").context("HOME não definido")?;
    Ok(PathBuf::from(home).join("Documentos/forja-cad"))
}

fn control_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("FORJA_SOLIDCRAFT_CONTROL") {
        if !dir.trim().is_empty() {
            return Some(PathBuf::from(dir));
        }
    }
    let home = std::env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".var/app/io.tangencylabs.solidcraft/data/solidcraft/control"))
}

/// Manda `doc.open` pelo protocolo de controle, sem esperar o app para sempre.
fn control(path: &Path) -> Control {
    let Some(dir) = control_dir() else {
        return Control::Unreachable("HOME indisponível para achar o controle".to_string());
    };
    let port = match std::env::var("FORJA_SOLIDCRAFT_PORT")
        .ok()
        .filter(|p| !p.trim().is_empty())
    {
        Some(port) => port,
        None => match std::fs::read_to_string(dir.join("current.port")) {
            Ok(text) => text.trim().to_string(),
            Err(_) => return Control::Unreachable("o SolidCraft não está rodando".to_string()),
        },
    };
    let Ok(port_number) = port.parse::<u16>() else {
        return Control::Unreachable(format!("porta de controle inválida: {port}"));
    };
    let token = match std::env::var("FORJA_SOLIDCRAFT_TOKEN")
        .ok()
        .filter(|t| !t.trim().is_empty())
    {
        Some(token) => token,
        None => match std::fs::read_to_string(dir.join(format!("{port}.token"))) {
            Ok(text) => text.trim().to_string(),
            Err(_) => return Control::Refused("sem o token de controle do app".to_string()),
        },
    };

    let address = match ("127.0.0.1", port_number)
        .to_socket_addrs()
        .ok()
        .and_then(|mut a| a.next())
    {
        Some(address) => address,
        None => return Control::Unreachable(format!("endereço de controle inválido: {port}")),
    };
    let mut stream = match TcpStream::connect_timeout(&address, Duration::from_millis(800)) {
        Ok(stream) => stream,
        Err(_) => return Control::Unreachable("o SolidCraft não está rodando".to_string()),
    };
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));

    let request = json!({
        "id": 1,
        "token": token,
        "method": "engine.execute",
        "params": {
            "allow_paths": [path.parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default()],
            "command": "doc.open",
            "params": { "path": path.to_string_lossy() },
        },
    });
    let mut line = request.to_string();
    line.push('\n');
    if let Err(error) = stream.write_all(line.as_bytes()) {
        return Control::Refused(format!("não consegui falar com o app: {error}"));
    }
    let mut reply = String::new();
    match BufReader::new(stream).read_line(&mut reply) {
        Ok(0) => Control::Refused("o app fechou a conexão sem responder".to_string()),
        Ok(_) => match serde_json::from_str::<Value>(reply.trim()) {
            Ok(value) if value.get("ok") == Some(&Value::Bool(true)) => Control::Opened,
            Ok(value) => Control::Refused(
                value
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("o app recusou a abertura")
                    .to_string(),
            ),
            Err(_) => Control::Refused(format!("resposta inesperada do app: {}", reply.trim())),
        },
        Err(_) => Control::Refused(
            "o app não respondeu em 5 s (a janela pode estar ocupada com um diálogo)".to_string(),
        ),
    }
}

/// Abre o app com o arquivo. O processo é esperado numa thread para o serviço
/// não acumular zumbis.
fn launch(path: &Path) -> Result<()> {
    let mut child = std::process::Command::new("flatpak")
        .args(["run", "io.tangencylabs.solidcraft"])
        .arg(path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .context("executar `flatpak run io.tangencylabs.solidcraft`")?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}
