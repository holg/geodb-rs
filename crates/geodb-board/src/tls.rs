//! TLS for the bridge (`wss://`), so a page from an https site (trahe.eu) reaches it: browsers let an
//! https page open secure WebSockets only.
//!
//! The certificates are made here with `openssl`, in `crates/geodb-board/.tls/` (git-ignored):
//!
//! - `ca.pem`: a small certificate authority of this host, made once (10 years). Installed and
//!   trusted on a phone once (the bridge serves it as `/geodb-bridge-ca.cer`), it makes every
//!   later server certificate trusted, including after the addresses change.
//! - `server.pem`: for `localhost`, this host's names and its current IPv4 addresses, signed by
//!   the CA (825 days, `serverAuth`: what iOS asks of a TLS server certificate). Made again when
//!   the addresses change.

use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::ServerConfig;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

pub fn dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/.tls"))
}

/// This host's IPv4 addresses (not loopback), from `ifconfig`.
pub fn local_addresses() -> Vec<String> {
    let out = Command::new("ifconfig")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    out.lines()
        .filter_map(|l| l.trim().strip_prefix("inet "))
        .filter_map(|l| l.split_whitespace().next())
        .map(|a| a.trim_start_matches("addr:").to_string())
        .filter(|a| a != "127.0.0.1")
        .collect()
}

fn hostname() -> String {
    Command::new("hostname")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "localhost".into())
}

fn openssl(args: &[&str]) -> Result<(), String> {
    let out = Command::new("openssl")
        .args(args)
        .output()
        .map_err(|e| format!("openssl: {e} (needed once to make the certificates)"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "openssl {}: {}",
            args.first().unwrap_or(&""),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

fn path(dir: &Path, name: &str) -> String {
    dir.join(name).to_string_lossy().into_owned()
}

/// Makes the CA (once) and the server certificate (when the names changed); returns the names.
fn ensure(dir: &Path) -> Result<Vec<String>, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let host = hostname();
    if !dir.join("ca.pem").exists() {
        openssl(&[
            "req",
            "-x509",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-days",
            "3650",
            "-keyout",
            &path(dir, "ca.key"),
            "-out",
            &path(dir, "ca.pem"),
            "-subj",
            &format!("/CN=geodb bridge CA ({host})"),
            "-addext",
            "basicConstraints=critical,CA:TRUE",
            "-addext",
            "keyUsage=critical,keyCertSign,cRLSign",
        ])?;
        openssl(&[
            "x509",
            "-in",
            &path(dir, "ca.pem"),
            "-outform",
            "der",
            "-out",
            &path(dir, "ca.cer"),
        ])?;
        println!(
            "made a certificate authority for this host: {}",
            dir.join("ca.pem").display()
        );
    }
    let short = host.split('.').next().unwrap_or("localhost").to_string();
    let mut dns = vec![
        "localhost".to_string(),
        host.clone(),
        format!("{short}.local"),
    ];
    dns.dedup();
    let mut ips = vec!["127.0.0.1".to_string()];
    ips.extend(local_addresses());
    let names: Vec<String> = dns.iter().chain(&ips).cloned().collect();
    let key = names.join(",");
    let made_for = std::fs::read_to_string(dir.join("server.names")).unwrap_or_default();
    if made_for.trim() != key || !dir.join("server.pem").exists() {
        let san: Vec<String> = dns
            .iter()
            .map(|d| format!("DNS:{d}"))
            .chain(ips.iter().map(|i| format!("IP:{i}")))
            .collect();
        let ext = format!(
            "basicConstraints=CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\n\
             extendedKeyUsage=serverAuth\nsubjectAltName={}\n",
            san.join(",")
        );
        std::fs::write(dir.join("server.ext"), ext).map_err(|e| e.to_string())?;
        openssl(&[
            "req",
            "-new",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-keyout",
            &path(dir, "server.key"),
            "-out",
            &path(dir, "server.csr"),
            "-subj",
            &format!("/CN={host}"),
        ])?;
        openssl(&[
            "x509",
            "-req",
            "-in",
            &path(dir, "server.csr"),
            "-CA",
            &path(dir, "ca.pem"),
            "-CAkey",
            &path(dir, "ca.key"),
            "-CAcreateserial",
            "-days",
            "825",
            "-sha256",
            "-extfile",
            &path(dir, "server.ext"),
            "-out",
            &path(dir, "server.pem"),
        ])?;
        std::fs::write(dir.join("server.names"), &key).map_err(|e| e.to_string())?;
        println!("made a server certificate for {key}");
    }
    Ok(names)
}

/// The TLS setup of the bridge, and the names its certificate covers.
pub fn config() -> Result<(Arc<ServerConfig>, Vec<String>), String> {
    let dir = dir();
    let names = ensure(&dir)?;
    let mut chain: Vec<CertificateDer<'static>> = Vec::new();
    for name in ["server.pem", "ca.pem"] {
        for cert in
            CertificateDer::pem_file_iter(dir.join(name)).map_err(|e| format!("{name}: {e}"))?
        {
            chain.push(cert.map_err(|e| format!("{name}: {e}"))?);
        }
    }
    let key = PrivateKeyDer::from_pem_file(dir.join("server.key"))
        .map_err(|e| format!("server.key: {e}"))?;
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .with_no_client_auth()
        .with_single_cert(chain, key)
        .map_err(|e| e.to_string())?;
    Ok((Arc::new(config), names))
}

/// The CA in DER (`.cer`): what a phone installs as a profile.
pub fn ca_der() -> Option<Vec<u8>> {
    std::fs::read(dir().join("ca.cer")).ok()
}
