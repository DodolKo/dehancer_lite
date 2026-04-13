use anyhow::{Context, Result};
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const DIST_DIR: &str = "dist";
const WASM_TARGET: &str = "wasm32-unknown-unknown";
const WASM_CRATE: &str = "dehancer-lite";
const WASM_BINDGEN_OUT_NAME: &str = "dehancer_lite";
const HOST_BASE: &str = "127.0.0.1";
const HOST_PORT_START: u16 = 8080;
const HOST_PORT_END: u16 = 8090;

fn main() -> ExitCode {
    let Some(cmd) = std::env::args().nth(1) else {
        eprintln!("Usage: cargo web-serve | cargo web-build");
        return ExitCode::from(2);
    };

    let result = match cmd.as_str() {
        "web-build" => build_web(),
        "web-serve" => serve_web(),
        other => {
            eprintln!("Unknown xtask command: {other}");
            return ExitCode::from(2);
        }
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err:?}");
            ExitCode::from(1)
        }
    }
}

fn build_web() -> Result<()> {
    build_wasm()?;
    let wasm_input = Path::new("target")
        .join(WASM_TARGET)
        .join("release")
        .join(format!("{WASM_CRATE}.wasm"));

    let stripped_wasm = Path::new("target")
        .join(WASM_TARGET)
        .join("release")
        .join(format!("{WASM_BINDGEN_OUT_NAME}_stripped.wasm"));
    strip_target_features(&wasm_input, &stripped_wasm)
        .with_context(|| format!("stripping target features from {}", wasm_input.display()))?;

    prepare_dist_dir()?;
    copy_static_assets(Path::new(DIST_DIR))?;
    run_wasm_bindgen(&stripped_wasm, Path::new(DIST_DIR))?;
    Ok(())
}

fn serve_web() -> Result<()> {
    build_web()?;

    let (listener, host) = bind_listener()?;
    println!("Serving {DIST_DIR} at http://{host}/");
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                if let Err(err) = handle_client(stream, Path::new(DIST_DIR)) {
                    eprintln!("request error: {err:#}");
                }
            }
            Err(err) => eprintln!("accept error: {err:#}"),
        }
    }

    Ok(())
}

fn bind_listener() -> Result<(TcpListener, String)> {
    for port in HOST_PORT_START..=HOST_PORT_END {
        let host = format!("{HOST_BASE}:{port}");
        match TcpListener::bind(&host) {
            Ok(listener) => return Ok((listener, host)),
            Err(err) if err.kind() == std::io::ErrorKind::AddrInUse => continue,
            Err(err) => return Err(err).with_context(|| format!("failed to bind to {host}")),
        }
    }

    anyhow::bail!(
        "no free port available in the range {HOST_PORT_START}..={HOST_PORT_END}"
    )
}

fn build_wasm() -> Result<()> {
    let status = Command::new("cargo")
        .args(["build", "--target", WASM_TARGET, "--release"])
        .status()
        .context("failed to execute cargo build")?;

    if !status.success() {
        anyhow::bail!("cargo build failed with status {status}");
    }

    Ok(())
}

fn run_wasm_bindgen(input: &Path, out_dir: &Path) -> Result<()> {
    let status = Command::new("wasm-bindgen")
        .args([
            "--target=web",
            "--out-dir",
            out_dir
                .to_str()
                .context("dist directory path is not valid UTF-8")?,
            "--out-name",
            WASM_BINDGEN_OUT_NAME,
            input
                .to_str()
                .context("stripped wasm path is not valid UTF-8")?,
            "--no-typescript",
        ])
        .status()
        .context("failed to execute wasm-bindgen")?;

    if !status.success() {
        anyhow::bail!("wasm-bindgen failed with status {status}");
    }

    Ok(())
}

fn prepare_dist_dir() -> Result<()> {
    let dist = Path::new(DIST_DIR);
    if dist.exists() {
        fs::remove_dir_all(dist).context("failed to clear dist directory")?;
    }
    fs::create_dir_all(dist).context("failed to create dist directory")?;
    Ok(())
}

fn copy_static_assets(dist: &Path) -> Result<()> {
    copy_file(Path::new("index.html"), &dist.join("index.html"))?;
    copy_dir(Path::new("web"), &dist.join("web"))?;
    Ok(())
}

fn copy_file(from: &Path, to: &Path) -> Result<()> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent).context("failed to create asset parent directory")?;
    }
    fs::copy(from, to)
        .with_context(|| format!("failed to copy {} to {}", from.display(), to.display()))?;
    Ok(())
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to).with_context(|| format!("failed to create {}", to.display()))?;
    for entry in fs::read_dir(from).with_context(|| format!("failed to read {}", from.display()))? {
        let entry = entry?;
        let src = entry.path();
        let dst = to.join(entry.file_name());
        if src.is_dir() {
            copy_dir(&src, &dst)?;
        } else {
            copy_file(&src, &dst)?;
        }
    }
    Ok(())
}

fn strip_target_features(input: &Path, output: &Path) -> Result<()> {
    let mut bytes = Vec::new();
    fs::File::open(input)
        .with_context(|| format!("failed to open {}", input.display()))?
        .read_to_end(&mut bytes)
        .with_context(|| format!("failed to read {}", input.display()))?;

    if bytes.len() < 8 || &bytes[..4] != b"\0asm" || bytes[4..8] != [1, 0, 0, 0] {
        anyhow::bail!("{} is not a valid wasm module", input.display());
    }

    let mut stripped = bytes[..8].to_vec();
    let mut cursor = 8usize;

    while cursor < bytes.len() {
        let section_start = cursor;
        let id = bytes[cursor];
        cursor += 1;
        let (payload_len, leb_len) = read_uleb(&bytes[cursor..])?;
        cursor += leb_len;
        let payload_start = cursor;
        let payload_end = payload_start
            .checked_add(payload_len as usize)
            .context("wasm section length overflow")?;
        if payload_end > bytes.len() {
            anyhow::bail!("truncated wasm section in {}", input.display());
        }

        let mut keep_section = true;
        if id == 0 {
            let (name, _) = read_name(&bytes[payload_start..payload_end])?;
            if name == "target_features" {
                keep_section = false;
            }
        }

        if keep_section {
            stripped.extend_from_slice(&bytes[section_start..payload_end]);
        }

        cursor = payload_end;
    }

    let mut file = fs::File::create(output)
        .with_context(|| format!("failed to create {}", output.display()))?;
    file.write_all(&stripped)
        .with_context(|| format!("failed to write {}", output.display()))?;
    Ok(())
}

fn read_name(input: &[u8]) -> Result<(String, usize)> {
    let (len, leb_len) = read_uleb(input)?;
    let start = leb_len;
    let end = start
        .checked_add(len as usize)
        .context("wasm name length overflow")?;
    if end > input.len() {
        anyhow::bail!("truncated wasm custom section name");
    }
    let name = std::str::from_utf8(&input[start..end])
        .context("wasm custom section name was not utf-8")?
        .to_owned();
    Ok((name, end))
}

fn read_uleb(input: &[u8]) -> Result<(u32, usize)> {
    let mut value = 0u32;
    let mut shift = 0u32;
    for (idx, byte) in input.iter().copied().enumerate() {
        let low = u32::from(byte & 0x7f);
        value |= low
            .checked_shl(shift)
            .context("uleb shift overflow")?;
        if byte & 0x80 == 0 {
            return Ok((value, idx + 1));
        }
        shift += 7;
        if shift >= 35 {
            anyhow::bail!("uleb128 value is too large");
        }
    }
    anyhow::bail!("truncated uleb128 value")
}

fn handle_client(mut stream: TcpStream, root: &Path) -> Result<()> {
    let mut request = [0u8; 4096];
    let len = stream
        .read(&mut request)
        .context("failed to read client request")?;
    if len == 0 {
        return Ok(());
    }

    let request = String::from_utf8_lossy(&request[..len]);
    let mut lines = request.lines();
    let Some(line) = lines.next() else {
        return Ok(());
    };
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let path = parts.next().unwrap_or("/");

    if method != "GET" && method != "HEAD" {
        write_response(&mut stream, 405, "Method Not Allowed", "text/plain", b"405")?;
        return Ok(());
    }

    let path = normalize_path(path);
    let file_path = if path == "/" {
        root.join("index.html")
    } else {
        root.join(path.trim_start_matches('/'))
    };

    let resolved = if file_path.is_dir() {
        file_path.join("index.html")
    } else {
        file_path
    };

    match fs::read(&resolved) {
        Ok(body) => {
            let content_type = content_type(&resolved);
            if method == "HEAD" {
                write_response(&mut stream, 200, "OK", content_type, &[])?;
            } else {
                write_response(&mut stream, 200, "OK", content_type, &body)?;
            }
        }
        Err(_) => {
            let body = b"404 Not Found";
            write_response(&mut stream, 404, "Not Found", "text/plain", body)?;
        }
    }

    Ok(())
}

fn write_response(
    stream: &mut TcpStream,
    status: u16,
    reason: &str,
    content_type: &str,
    body: &[u8],
) -> Result<()> {
    let headers = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream
        .write_all(headers.as_bytes())
        .context("failed to write response headers")?;
    stream
        .write_all(body)
        .context("failed to write response body")?;
    Ok(())
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()).unwrap_or_default() {
        "html" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "wasm" => "application/wasm",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        _ => "application/octet-stream",
    }
}

fn normalize_path(path: &str) -> String {
    let cleaned = path.split('?').next().unwrap_or("/");
    let mut normalized = PathBuf::new();
    for segment in cleaned.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                normalized.pop();
            }
            other => normalized.push(other),
        }
    }

    if normalized.as_os_str().is_empty() {
        "/".to_string()
    } else {
        format!("/{}", normalized.display())
    }
}
