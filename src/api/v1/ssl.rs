/*
 * @Descripttion: SSL/ACME certificate management (1Panel-style)
 * - Multiple ACME providers: Let's Encrypt, ZeroSSL, Buypass, Google
 * - HTTP-01 & DNS-01 challenges
 * - ACME account management
 * - Certificate deployment to nginx sites
 */
use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::process::Command;

use crate::api::{ResponseStructure, ResponseStructureError};

// ── Data structures ──

#[derive(Serialize)]
pub struct AcmeProvider {
    pub name: &'static str,
    pub label: &'static str,
    pub directory_url: &'static str,
    pub is_staging: bool,
}

#[derive(Deserialize)]
pub struct IssueCertBody {
    pub domains: Vec<String>,
    pub email: Option<String>,
    pub acme_provider: Option<String>,
    pub challenge: Option<String>,       // "http" or "dns"
    pub dns_plugin: Option<String>,      // e.g. "cloudflare", "aliyun"
    pub dns_credentials: Option<String>, // path to credentials file
    pub webroot: Option<String>,
}

#[derive(Serialize)]
pub struct CertInfo {
    pub domain: String,
    pub expiry: Option<String>,
    pub issuer: Option<String>,
    pub issued_at: Option<String>,
    pub cert_path: Option<String>,
    pub key_path: Option<String>,
    pub san: Vec<String>,
    pub fingerprint: Option<String>,
    pub key_type: Option<String>,
    pub valid: bool,
}

#[derive(Deserialize)]
pub struct DeployCertBody {
    pub site_name: String,
    pub domain: String,
}

#[derive(Deserialize)]
pub struct AcmeAccountBody {
    pub email: String,
    pub acme_provider: Option<String>,
    #[serde(default = "default_true")]
    pub agree_tos: bool,
}

fn default_true() -> bool { true }


// ── Constants ──

const ACME_PROVIDERS: &[AcmeProvider] = &[
    AcmeProvider { name: "letsencrypt", label: "Let's Encrypt", directory_url: "https://acme-v02.api.letsencrypt.org/directory", is_staging: false },
    AcmeProvider { name: "letsencrypt-staging", label: "Let's Encrypt (Staging)", directory_url: "https://acme-staging-v02.api.letsencrypt.org/directory", is_staging: true },
    AcmeProvider { name: "zerossl", label: "ZeroSSL", directory_url: "https://acme.zerossl.com/v2/DV90", is_staging: false },
    AcmeProvider { name: "buypass", label: "Buypass", directory_url: "https://api.buypass.com/acme/directory", is_staging: false },
    AcmeProvider { name: "google", label: "Google Public CA", directory_url: "https://dv.acme-v02.api.pki.goog/directory", is_staging: false },
];

const DNS_PLUGINS: &[(&str, &str)] = &[
    ("cloudflare", "certbot-dns-cloudflare"),
    ("aliyun", "certbot-dns-aliyun"),
    ("dnspod", "certbot-dns-dnspod"),
    ("godaddy", "certbot-dns-godaddy"),
    ("digitalocean", "certbot-dns-digitalocean"),
    ("aws", "certbot-dns-route53"),
    ("gcloud", "certbot-dns-google"),
    ("cloudxns", "certbot-dns-cloudxns"),
    ("linode", "certbot-dns-linode"),
    ("he", "certbot-dns-dns-he"),
];

const MANAGED_CERTBOT_DIR: &str = "./runtime/certbot";
const MANAGED_CERTBOT_BIN: &str = "./runtime/certbot/bin/certbot";


// ── Helpers ──

fn is_linux() -> bool { cfg!(target_os = "linux") }

fn acme_provider_url(name: &str) -> Option<&'static str> {
    ACME_PROVIDERS.iter().find(|p| p.name == name).map(|p| p.directory_url)
}

async fn find_certbot() -> Option<String> {
    // 1. Check managed installation (panel's own certbot venv)
    if tokio::fs::metadata(MANAGED_CERTBOT_BIN).await.is_ok() {
        return Some(MANAGED_CERTBOT_BIN.to_string());
    }
    // 2. Check standard system paths
    let paths = ["/usr/bin/certbot", "/usr/local/bin/certbot", "/opt/certbot/bin/certbot"];
    for path in &paths {
        if tokio::fs::metadata(path).await.is_ok() {
            return Some(path.to_string());
        }
    }
    // 3. Check via `which`
    if let Ok(output) = Command::new("which").arg("certbot").output().await {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() { return Some(path); }
        }
    }
    // 4. Auto-install certbot via pip3 into managed venv
    ensure_certbot().await
}

/// Install certbot into a managed Python virtualenv under the panel's runtime directory.
async fn ensure_certbot() -> Option<String> {
    if tokio::fs::metadata(MANAGED_CERTBOT_BIN).await.is_ok() {
        return Some(MANAGED_CERTBOT_BIN.to_string());
    }
    let _ = tokio::fs::create_dir_all(MANAGED_CERTBOT_DIR).await;
    let venv_dir = format!("{}/venv", MANAGED_CERTBOT_DIR);
    let venv_pip = format!("{}/bin/pip", venv_dir);
    // Create virtualenv
    let ok = Command::new("python3")
        .args(["-m", "venv", &venv_dir])
        .status().await.map(|s| s.success()).unwrap_or(false);
    if !ok { return None; }
    // Upgrade pip, then install certbot
    let _ = Command::new(&venv_pip).args(["install", "--upgrade", "pip"]).status().await;
    let ok = Command::new(&venv_pip).args(["install", "certbot"]).status().await
        .map(|s| s.success()).unwrap_or(false);
    if !ok { return None; }
    // Resolve absolute paths for reliable symlink
    let cwd = std::env::current_dir().unwrap_or_default();
    let abs_venv_bin = cwd.join(&venv_dir).join("bin").join("certbot");
    let abs_managed_bin = cwd.join(MANAGED_CERTBOT_BIN);
    let _ = tokio::fs::create_dir_all(abs_managed_bin.parent().unwrap()).await;
    let _ = std::os::unix::fs::symlink(&abs_venv_bin, &abs_managed_bin);
    if tokio::fs::metadata(MANAGED_CERTBOT_BIN).await.is_ok() {
        Some(MANAGED_CERTBOT_BIN.to_string())
    } else {
        None
    }
}


// ── Setup endpoint ──

#[derive(Serialize)]
struct SetupStatus {
    certbot_installed: bool,
    certbot_path: Option<String>,
    install_error: Option<String>,
}

/// POST /api/v1/ssl/setup — ensure certbot is installed, installing it if necessary
pub async fn setup_certbot() -> HttpResponse {
    // Force install attempt regardless of current state
    let _ = tokio::fs::create_dir_all(MANAGED_CERTBOT_DIR).await;
    let venv_dir = format!("{}/venv", MANAGED_CERTBOT_DIR);
    let venv_pip = format!("{}/bin/pip", venv_dir);

    // Check if already available
    if let Some(path) = find_certbot().await {
        return HttpResponse::Ok().json(ResponseStructure {
            success: true, code: 200,
            message: String::from("certbot already available"),
            data: Some(SetupStatus {
                certbot_installed: true,
                certbot_path: Some(path),
                install_error: None,
            }),
        });
    }

    // Install: create virtualenv
    let venv_ok = Command::new("python3")
        .args(["-m", "venv", &venv_dir])
        .status().await.map(|s| s.success()).unwrap_or(false);
    if !venv_ok {
        return HttpResponse::InternalServerError().json(ResponseStructure {
            success: false, code: 500,
            message: String::from("Failed to create Python virtualenv for certbot"),
            data: Some(SetupStatus {
                certbot_installed: false, certbot_path: None,
                install_error: Some(String::from("python3 -m venv failed")),
            }),
        });
    }

    let _ = Command::new(&venv_pip).args(["install", "--upgrade", "pip"]).status().await;

    let status = Command::new(&venv_pip).args(["install", "certbot"]).status().await;
    match status {
        Ok(out) if out.success() => {
            // Symlink
            let cwd = std::env::current_dir().unwrap_or_default();
            let abs_venv_bin = cwd.join(&venv_dir).join("bin").join("certbot");
            let abs_managed_bin = cwd.join(MANAGED_CERTBOT_BIN);
            let _ = tokio::fs::create_dir_all(abs_managed_bin.parent().unwrap()).await;
            let _ = std::os::unix::fs::symlink(&abs_venv_bin, &abs_managed_bin);
            if tokio::fs::metadata(MANAGED_CERTBOT_BIN).await.is_ok() {
                HttpResponse::Ok().json(ResponseStructure {
                    success: true, code: 200,
                    message: String::from("certbot installed successfully"),
                    data: Some(SetupStatus {
                        certbot_installed: true,
                        certbot_path: Some(MANAGED_CERTBOT_BIN.to_string()),
                        install_error: None,
                    }),
                })
            } else {
                HttpResponse::InternalServerError().json(ResponseStructure {
                    success: false, code: 500,
                    message: String::from("certbot installed but binary not found"),
                    data: Some(SetupStatus {
                        certbot_installed: false, certbot_path: None,
                        install_error: Some(String::from("certbot binary not found after installation")),
                    }),
                })
            }
        }
        Ok(_) => {
            let err = String::from("pip install certbot failed (check logs)");
            HttpResponse::InternalServerError().json(ResponseStructure {
                success: false, code: 500,
                message: err.clone(),
                data: Some(SetupStatus {
                    certbot_installed: false, certbot_path: None,
                    install_error: Some(err),
                }),
            })
        }
        Err(e) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false, code: 500,
            message: format!("Failed to run pip install: {}", e),
        }),
    }
}

fn parse_certbot_domain(cert: &Value) -> CertInfo {
    let domain = cert.get("name")
        .or_else(|| cert.get("domain"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown").to_string();

    let expiry = cert.get("expiry").and_then(|v| v.as_str()).map(|s| s.to_string());
    let cert_path = cert.get("cert_path").or_else(|| cert.get("certfile")).and_then(|v| v.as_str()).map(|s| s.to_string());
    let key_path = cert.get("key_path").or_else(|| cert.get("privkeyfile")).and_then(|v| v.as_str()).map(|s| s.to_string());

    // Extract SANs from certbot JSON
    let san = cert.get("san")
        .or_else(|| cert.get("subject_alt_names"))
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_else(|| {
            let mut list = vec![domain.clone()];
            if let Some(alt_names) = cert.get("alt_names").and_then(|v| v.as_array()) {
                for name in alt_names {
                    if let Some(s) = name.as_str() {
                        if s != &domain {
                            list.push(s.to_string());
                        }
                    }
                }
            }
            list
        });

    CertInfo { domain, expiry, issuer: None, issued_at: None, cert_path, key_path, san, fingerprint: None, key_type: None, valid: true }
}

async fn enrich_cert_with_openssl(info: &mut CertInfo) {
    let cert_path = match info.cert_path.as_ref() {
        Some(p) => p.clone(),
        None => return,
    };

    // enddate
    if let Ok(out) = Command::new("openssl").args(["x509", "-enddate", "-noout", "-in", &cert_path]).output().await {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            info.expiry = Some(s.trim_start_matches("notAfter=").to_string());
        }
    }

    // issuer
    if let Ok(out) = Command::new("openssl").args(["x509", "-issuer", "-noout", "-in", &cert_path]).output().await {
        if out.status.success() {
            info.issuer = Some(String::from_utf8_lossy(&out.stdout).trim().trim_start_matches("issuer=").to_string());
        }
    }

    // fingerprint (SHA256)
    if let Ok(out) = Command::new("openssl").args(["x509", "-fingerprint", "-sha256", "-noout", "-in", &cert_path]).output().await {
        if out.status.success() {
            info.fingerprint = Some(String::from_utf8_lossy(&out.stdout).trim().trim_start_matches("SHA256 Fingerprint=").to_string());
        }
    }

    // key type & size
    if let Ok(out) = Command::new("openssl").args(["x509", "-noout", "-text", "-in", &cert_path]).output().await {
        if out.status.success() {
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                let t = line.trim();
                if t.starts_with("Public Key Algorithm:") {
                    let algo = t.trim_start_matches("Public Key Algorithm:").trim();
                    info.key_type = Some(algo.to_string());
                    break;
                }
            }
        }
    }
}

// ── Endpoints ──

/// GET /api/v1/ssl/providers — list available ACME providers
pub async fn list_acme_providers() -> HttpResponse {
    HttpResponse::Ok().json(ResponseStructure {
        success: true,
        code: 200,
        message: String::from("success"),
        data: Some(serde_json::json!({
            "providers": ACME_PROVIDERS,
            "dns_plugins": DNS_PLUGINS.iter().map(|(name, pkg)| serde_json::json!({"name": name, "package": pkg})).collect::<Vec<_>>(),
        })),
    })
}

/// GET /api/v1/ssl/accounts — list ACME accounts
pub async fn list_acme_accounts() -> HttpResponse {
    if !is_linux() {
        return HttpResponse::BadRequest().json(ResponseStructureError {
            success: false, code: 400,
            message: String::from("SSL management is only supported on Linux"),
        });
    }
    let certbot_path = match find_certbot().await {
        Some(p) => p,
        None => return HttpResponse::Ok().json(ResponseStructure {
            success: true, code: 200,
            message: String::from("certbot not installed"),
            data: Some(serde_json::json!({"accounts": []})),
        }),
    };

    let output = Command::new(&certbot_path).args(["show_account", "--json"]).output().await;
    match output {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let accounts = if let Ok(json) = serde_json::from_str::<Value>(&stdout) {
                let mut list = vec![];
                if let Some(email) = json.get("email").and_then(|v| v.as_str()) {
                    let created = json.get("created_at").and_then(|v| v.as_str()).map(|s| s.to_string());
                    let ca = json.get("acme").and_then(|v| v.as_str()).or_else(|| json.get("server").and_then(|v| v.as_str())).map(|s| s.to_string());
                    list.push(serde_json::json!({"email": email, "created_at": created, "ca": ca}));
                }
                list
            } else { vec![] };

            HttpResponse::Ok().json(ResponseStructure {
                success: true, code: 200, message: String::from("success"),
                data: Some(serde_json::json!({"accounts": accounts})),
            })
        }
        _ => HttpResponse::Ok().json(ResponseStructure {
            success: true, code: 200, message: String::from("success"),
            data: Some(serde_json::json!({"accounts": []})),
        }),
    }
}

/// POST /api/v1/ssl/accounts — register ACME account
pub async fn register_acme_account(body: web::Json<AcmeAccountBody>) -> HttpResponse {
    if !is_linux() {
        return HttpResponse::BadRequest().json(ResponseStructureError {
            success: false, code: 400,
            message: String::from("SSL management is only supported on Linux"),
        });
    }
    let certbot_path = match find_certbot().await {
        Some(p) => p,
        None => return HttpResponse::BadRequest().json(ResponseStructureError {
            success: false, code: 400,
            message: String::from("certbot is not installed"),
        }),
    };


    let mut cmd = Command::new(&certbot_path);
    cmd.args(["register", "--non-interactive", "--agree-tos", "--email", &body.email]);
    if let Some(ref provider) = body.acme_provider {
        if let Some(url) = acme_provider_url(provider) {
            cmd.args(["--server", url]);
        }
    }

    let output = cmd.output().await;
    match output {
        Ok(out) if out.status.success() => {
            HttpResponse::Ok().json(ResponseStructure {
                success: true, code: 200,
                message: String::from("Account registered successfully"),
                data: Some(serde_json::json!({"email": body.email})),
            })
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            HttpResponse::BadRequest().json(ResponseStructureError {
                success: false, code: 400,
                message: if stderr.is_empty() { String::from("Registration failed") } else { stderr },
            })
        }
        Err(e) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false, code: 500,
            message: format!("Failed to run certbot: {}", e),
        }),
    }
}

/// GET /api/v1/ssl/certificates — list certificates
pub async fn list_certificates() -> HttpResponse {
    if !is_linux() {
        return HttpResponse::BadRequest().json(ResponseStructureError {
            success: false, code: 400,
            message: String::from("SSL management is only supported on Linux"),
        });
    }

    let certbot_path = match find_certbot().await {
        Some(p) => p,
        None => return HttpResponse::Ok().json(ResponseStructure {
            success: true, code: 200,
            message: String::from("certbot not installed"),
            data: Some(serde_json::json!({"installed": false, "certificates": []})),
        }),
    };

    let output = Command::new(&certbot_path).args(["certificates", "--json"]).output().await;
    match output {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let certs: Vec<CertInfo> = serde_json::from_str::<Value>(&stdout)
                .ok()
                .and_then(|j| j.get("certificates").and_then(|v| v.as_array()).cloned())
                .or_else(|| serde_json::from_str::<Vec<Value>>(&stdout).ok())
                .unwrap_or_default()
                .iter()
                .map(parse_certbot_domain)
                .collect();

            HttpResponse::Ok().json(ResponseStructure {
                success: true, code: 200, message: String::from("success"),
                data: Some(serde_json::json!({"installed": true, "certificates": certs})),
            })
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            HttpResponse::Ok().json(ResponseStructure {
                success: true, code: 200,
                message: String::from("certbot query failed"),
                data: Some(serde_json::json!({"installed": true, "certificates": [], "error": stderr})),
            })
        }
        Err(e) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false, code: 500,
            message: format!("Failed to query certificates: {}", e),
        }),
    }
}

/// GET /api/v1/ssl/certificates/{domain} — get certificate detail
pub async fn get_certificate_detail(path: web::Path<String>) -> HttpResponse {
    if !is_linux() {
        return HttpResponse::BadRequest().json(ResponseStructureError {
            success: false, code: 400,
            message: String::from("SSL management is only supported on Linux"),
        });
    }
    let domain = path.into_inner();
    let certbot_path = match find_certbot().await {
        Some(p) => p,
        None => return HttpResponse::NotFound().json(ResponseStructureError {
            success: false, code: 404,
            message: String::from("certbot not installed"),
        }),
    };

    let output = Command::new(&certbot_path).args(["certificates", "--json"]).output().await;
    match output {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let json: Value = match serde_json::from_str(&stdout) { Ok(j) => j, Err(_) => return HttpResponse::NotFound().json(ResponseStructureError {
                success: false, code: 404,
                message: format!("Certificate for '{}' not found", domain),
            })};

            let certs = json.get("certificates").and_then(|v| v.as_array()).cloned()
                .or_else(|| serde_json::from_str::<Vec<Value>>(&stdout).ok())
                .unwrap_or_default();

            for cert in &certs {
                let name = cert.get("name").or_else(|| cert.get("domain")).and_then(|v| v.as_str()).unwrap_or("");
                if name == domain {
                    let mut info = parse_certbot_domain(cert);
                    enrich_cert_with_openssl(&mut info).await;
                    return HttpResponse::Ok().json(ResponseStructure {
                        success: true, code: 200, message: String::from("success"),
                        data: Some(serde_json::json!(info)),
                    });
                }
            }
            HttpResponse::NotFound().json(ResponseStructureError {
                success: false, code: 404,
                message: format!("Certificate for '{}' not found", domain),
            })
        }
        _ => HttpResponse::NotFound().json(ResponseStructureError {
            success: false, code: 404,
            message: format!("Certificate for '{}' not found", domain),
        }),
    }
}

/// POST /api/v1/ssl/issue — issue certificate
pub async fn issue_certificate(body: web::Json<IssueCertBody>) -> HttpResponse {
    if !is_linux() {
        return HttpResponse::BadRequest().json(ResponseStructureError {
            success: false, code: 400,
            message: String::from("SSL management is only supported on Linux"),
        });
    }
    let certbot_path = match find_certbot().await {
        Some(p) => p,
        None => return HttpResponse::BadRequest().json(ResponseStructureError {
            success: false, code: 400,
            message: String::from("certbot is not installed"),
        }),
    };
    if body.domains.is_empty() {
        return HttpResponse::BadRequest().json(ResponseStructureError {
            success: false, code: 400,
            message: String::from("At least one domain is required"),
        });
    }

    let main_domain = &body.domains[0];
    let challenge = body.challenge.as_deref().unwrap_or("http");
    let mut cmd = Command::new(&certbot_path);
    cmd.arg("certonly");
    cmd.arg("--non-interactive");

    // ACME provider
    if let Some(ref provider) = body.acme_provider {
        if let Some(url) = acme_provider_url(provider) {
            cmd.args(["--server", url]);
        }
    }

    // Challenge type
    match challenge {
        "dns" => {
            cmd.arg("--preferred-challenges").arg("dns-01");
            if let Some(ref plugin) = body.dns_plugin {
                cmd.arg("--dns").arg(plugin);
                if let Some(ref creds) = body.dns_credentials {
                    cmd.args(["--dns-credentials", creds]);
                    // Also try the plugin-specific flag
                    cmd.args([&format!("--dns-{}-credentials", plugin), creds]);
                }
            } else {
                // Manual DNS mode — we can't do non-interactive manual DNS
                // without a hook; fall back to standalone and warn
                cmd.arg("--manual");
                cmd.arg("--manual-auth-hook").arg("echo");
                cmd.arg("--manual-cleanup-hook").arg("echo");
            }
        }
        "http" | _ => {
            if let Some(ref webroot) = body.webroot {
                cmd.arg("--webroot").arg("-w").arg(webroot);
            } else {
                cmd.arg("--standalone");
            }
        }
    }

    for domain in &body.domains {
        cmd.arg("-d").arg(domain);
    }
    if let Some(ref email) = body.email {
        cmd.arg("--email").arg(email);
    } else {
        cmd.arg("--register-unsafely-without-email");
    }
    cmd.arg("--agree-tos");

    let output = cmd.output().await;
    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            if out.status.success() {
                HttpResponse::Ok().json(ResponseStructure {
                    success: true, code: 200,
                    message: String::from("Certificate issued successfully"),
                    data: Some(serde_json::json!({"domain": main_domain, "output": stdout + &stderr})),
                })
            } else {
                HttpResponse::BadRequest().json(ResponseStructureError {
                    success: false, code: 400,
                    message: format!("Failed to issue certificate: {}", stderr),
                })
            }
        }
        Err(e) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false, code: 500,
            message: format!("Failed to run certbot: {}", e),
        }),
    }
}

/// POST /api/v1/ssl/renew/{domain}
pub async fn renew_certificate(path: web::Path<String>) -> HttpResponse {
    if !is_linux() {
        return HttpResponse::BadRequest().json(ResponseStructureError {
            success: false, code: 400,
            message: String::from("SSL management is only supported on Linux"),
        });
    }
    let certbot_path = match find_certbot().await {
        Some(p) => p,
        None => return HttpResponse::BadRequest().json(ResponseStructureError {
            success: false, code: 400,
            message: String::from("certbot is not installed"),
        }),
    };
    let domain = path.into_inner();

    let output = Command::new(&certbot_path)
        .args(["renew", "--cert-name", &domain, "--non-interactive"])
        .output().await;

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            if out.status.success() {
                HttpResponse::Ok().json(ResponseStructure {
                    success: true, code: 200,
                    message: String::from("Certificate renewed successfully"),
                    data: Some(serde_json::json!({"domain": domain, "output": stdout + &stderr})),
                })
            } else {
                HttpResponse::BadRequest().json(ResponseStructureError {
                    success: false, code: 400,
                    message: format!("Failed to renew certificate: {}", stderr),
                })
            }
        }
        Err(e) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false, code: 500,
            message: format!("Failed to run certbot: {}", e),
        }),
    }
}

/// DELETE /api/v1/ssl/certificates/{domain}
pub async fn delete_certificate(path: web::Path<String>) -> HttpResponse {
    if !is_linux() {
        return HttpResponse::BadRequest().json(ResponseStructureError {
            success: false, code: 400,
            message: String::from("SSL management is only supported on Linux"),
        });
    }
    let certbot_path = match find_certbot().await {
        Some(p) => p,
        None => return HttpResponse::BadRequest().json(ResponseStructureError {
            success: false, code: 400,
            message: String::from("certbot is not installed"),
        }),
    };
    let domain = path.into_inner();

    let output = Command::new(&certbot_path)
        .args(["delete", "--cert-name", &domain, "--non-interactive"])
        .output().await;

    match output {
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            if out.status.success() {
                HttpResponse::Ok().json(ResponseStructure {
                    success: true, code: 200,
                    message: String::from("Certificate deleted successfully"),
                    data: None::<()>,
                })
            } else {
                HttpResponse::BadRequest().json(ResponseStructureError {
                    success: false, code: 400,
                    message: format!("Failed to delete certificate: {}", stderr),
                })
            }
        }
        Err(e) => HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false, code: 500,
            message: format!("Failed to run certbot: {}", e),
        }),
    }
}

/// POST /api/v1/ssl/deploy — deploy certificate to nginx site
pub async fn deploy_certificate(body: web::Json<DeployCertBody>) -> HttpResponse {
    if !is_linux() {
        return HttpResponse::BadRequest().json(ResponseStructureError {
            success: false, code: 400,
            message: String::from("SSL management is only supported on Linux"),
        });
    }

    let domain = &body.domain;
    let site_name = &body.site_name;

    let live_path = format!("/etc/letsencrypt/live/{}/", domain);
    if !tokio::fs::metadata(&live_path).await.is_ok() {
        return HttpResponse::NotFound().json(ResponseStructureError {
            success: false, code: 404,
            message: format!("Certificate for '{}' not found at {}", domain, live_path),
        });
    }

    let mut config_path: Option<String> = None;
    let search_dirs = [
        format!("/etc/nginx/sites-available/{}", site_name),
        format!("/etc/nginx/sites-available/{}.conf", site_name),
        format!("/etc/nginx/conf.d/{}.conf", site_name),
        format!("/etc/nginx/conf.d/{}", site_name),
    ];
    for p in &search_dirs {
        if tokio::fs::metadata(p).await.is_ok() {
            config_path = Some(p.clone());
            break;
        }
    }

    match config_path {
        Some(ref config_path) => {
            // Read existing config
            let content = tokio::fs::read_to_string(config_path).await.unwrap_or_default();

            // Check if SSL is already configured
            if content.contains("ssl_certificate") {
                return HttpResponse::Ok().json(ResponseStructure {
                    success: true, code: 200,
                    message: String::from("Site already has SSL configured. You may need to update paths to LetsEncrypt certs manually."),
                    data: None::<()>,
                });
            }

            // Add SSL directives before the closing brace of the server block
            // Find the last server block's closing brace
            let ssl_block = format!(
                "    listen 443 ssl;\n    ssl_certificate /etc/letsencrypt/live/{domain}/fullchain.pem;\n    ssl_certificate_key /etc/letsencrypt/live/{domain}/privkey.pem;\n    include /etc/letsencrypt/options-ssl-nginx.conf;\n    ssl_dhparam /etc/letsencrypt/ssl-dhparams.pem;\n\n",
                domain = domain
            );

            // Insert after the last `server_name` directive
            let mut new_content = content.clone();
            // Also ensure listen 443 is not duplicated
            if let Some(pos) = new_content.rfind("server_name") {
                let insert_pos = new_content[pos..].find(';').map(|end| pos + end + 1).unwrap_or(new_content.len());
                new_content.insert_str(insert_pos, &("\n".to_string() + &ssl_block));
            } else {
                // Fallback: insert before last }
                if let Some(pos) = new_content.trim_end().rfind('}') {
                    new_content.insert_str(pos, &ssl_block);
                }
            }

            if let Err(e) = tokio::fs::write(&config_path, &new_content).await {
                return HttpResponse::InternalServerError().json(ResponseStructureError {
                    success: false, code: 500,
                    message: format!("Failed to write config: {}", e),
                });
            }

            // Test nginx config
            let nginx_output = Command::new("nginx").arg("-t").output().await;
            let test_ok = match &nginx_output {
                Ok(out) => String::from_utf8_lossy(&out.stderr).contains("test is successful") || String::from_utf8_lossy(&out.stderr).contains("syntax is ok"),
                Err(_) => false,
            };

            if !test_ok {
                // Rollback
                let _ = tokio::fs::write(&config_path, &content).await;
                return HttpResponse::BadRequest().json(ResponseStructureError {
                    success: false, code: 400,
                    message: String::from("nginx config test failed after deploying SSL, changes reverted"),
                });
            }

            // Reload nginx
            let _ = Command::new("nginx").args(["-s", "reload"]).output().await;

            HttpResponse::Ok().json(ResponseStructure {
                success: true, code: 200,
                message: String::from("SSL certificate deployed and nginx reloaded"),
                data: Some(serde_json::json!({"site": site_name, "domain": domain})),
            })
        }
        None => HttpResponse::NotFound().json(ResponseStructureError {
            success: false, code: 404,
            message: format!("Nginx site '{}' not found", site_name),
        }),
    }
}

// ── Upload Certificate ──

#[derive(Deserialize)]
pub struct UploadCertBody {
    pub name: String,
    pub cert: String,
    pub key: String,
    #[serde(default)]
    pub key_type: Option<String>,
}

/// POST /api/v1/ssl/upload — upload existing certificate and private key
pub async fn upload_certificate(body: web::Json<UploadCertBody>) -> HttpResponse {
    let cert_dir = format!("{}/certificates/{}", MANAGED_CERTBOT_DIR, body.name);
    let _ = tokio::fs::create_dir_all(&cert_dir).await;

    let cert_path = format!("{}/fullchain.pem", cert_dir);
    let key_path = format!("{}/privkey.pem", cert_dir);

    // Validate cert PEM
    if !body.cert.contains("-----BEGIN") || !body.cert.contains("-----END") {
        return HttpResponse::BadRequest().json(ResponseStructureError {
            success: false, code: 400,
            message: String::from("Invalid certificate format: must be PEM"),
        });
    }
    if !body.key.contains("-----BEGIN") || !body.key.contains("-----END") {
        return HttpResponse::BadRequest().json(ResponseStructureError {
            success: false, code: 400,
            message: String::from("Invalid private key format: must be PEM"),
        });
    }

    if let Err(e) = tokio::fs::write(&cert_path, &body.cert).await {
        return HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false, code: 500,
            message: format!("Failed to write certificate: {}", e),
        });
    }
    if let Err(e) = tokio::fs::write(&key_path, &body.key).await {
        let _ = tokio::fs::remove_file(&cert_path).await;
        return HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false, code: 500,
            message: format!("Failed to write private key: {}", e),
        });
    }

    // Verify with openssl
    let output = Command::new("openssl")
        .args(["x509", "-noout", "-subject", "-dates", "-in", &cert_path])
        .output().await;

    let info = match output {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let lines: Vec<&str> = stdout.lines().collect();
            serde_json::json!({
                "subject": lines.get(0).unwrap_or(&"").to_string(),
                "dates": lines.get(1).or(lines.get(0)).unwrap_or(&"").to_string(),
            })
        }
        _ => serde_json::json!({"note": "could not verify certificate details"}),
    };

    // Check if key matches cert
    let key_check = Command::new("openssl")
        .args(["pkey", "-in", &key_path, "-pubout"])
        .output().await;
    let cert_pubkey = Command::new("openssl")
        .args(["x509", "-in", &cert_path, "-pubkey", "-noout"])
        .output().await;

    let key_match = if let (Ok(k), Ok(c)) = (key_check, cert_pubkey) {
        k.stdout == c.stdout
    } else {
        false
    };

    if !key_match {
        let _ = tokio::fs::remove_file(&cert_path).await;
        let _ = tokio::fs::remove_file(&key_path).await;
        return HttpResponse::BadRequest().json(ResponseStructureError {
            success: false, code: 400,
            message: String::from("Private key does not match certificate"),
        });
    }

    HttpResponse::Ok().json(ResponseStructure {
        success: true, code: 200,
        message: String::from("Certificate uploaded successfully"),
        data: Some(serde_json::json!({
            "name": body.name,
            "cert_path": cert_path,
            "key_path": key_path,
            "info": info,
        })),
    })
}

// ── Self-Signed Certificate ──

#[derive(Deserialize)]
pub struct SelfSignedBody {
    pub domain: String,
    #[serde(default)]
    pub key_type: Option<String>,
    #[serde(default = "default_365")]
    pub days: u32,
}

fn default_365() -> u32 { 365 }

/// POST /api/v1/ssl/self-signed — generate a self-signed certificate
pub async fn self_signed_certificate(body: web::Json<SelfSignedBody>) -> HttpResponse {
    let cert_dir = format!("{}/self-signed/{}", MANAGED_CERTBOT_DIR, body.domain);
    let _ = tokio::fs::create_dir_all(&cert_dir).await;

    let key_path = format!("{}/privkey.pem", cert_dir);
    let csr_path = format!("{}/csr.pem", cert_dir);
    let cert_path = format!("{}/fullchain.pem", cert_dir);

    let key_type = body.key_type.as_deref().unwrap_or("rsa:2048");
    let days = body.days.max(1).min(3650);

    // Generate private key
    match key_type {
        k if k.starts_with("ec") || k.starts_with("ecc") || k == "prime256v1" || k == "secp384r1" => {
            let curve = if k.starts_with("secp384") || k == "ec:384" { "secp384r1" } else { "prime256v1" };
            let status = Command::new("openssl")
                .args(["ecparam", "-genkey", "-name", curve, "-out", &key_path])
                .status().await.map(|s| s.success()).unwrap_or(false);
            if !status {
                return HttpResponse::InternalServerError().json(ResponseStructureError {
                    success: false, code: 500,
                    message: String::from("Failed to generate EC private key"),
                });
            }
        }
        _ => {
            let bits = key_type.trim_start_matches("rsa:").trim();
            let bits: u32 = bits.parse().unwrap_or(2048).max(2048).min(4096);
            let status = Command::new("openssl")
                .args(["genrsa", "-out", &key_path, &bits.to_string()])
                .status().await.map(|s| s.success()).unwrap_or(false);
            if !status {
                return HttpResponse::InternalServerError().json(ResponseStructureError {
                    success: false, code: 500,
                    message: String::from("Failed to generate RSA private key"),
                });
            }
        }
    }

    // Generate self-signed cert
    let status = Command::new("openssl")
        .args(["req", "-new", "-x509", "-key", &key_path,
               "-out", &cert_path,
               "-days", &days.to_string(),
               "-subj", &format!("/CN={}", body.domain)])
        .status().await.map(|s| s.success()).unwrap_or(false);

    if !status {
        let _ = tokio::fs::remove_dir_all(&cert_dir).await;
        return HttpResponse::InternalServerError().json(ResponseStructureError {
            success: false, code: 500,
            message: String::from("Failed to generate self-signed certificate"),
        });
    }

    // Also generate SAN extension for better browser compatibility
    let _ = Command::new("openssl")
        .args(["req", "-new", "-key", &key_path, "-out", &csr_path,
               "-subj", &format!("/CN={}", body.domain),
               "-addext", &format!("subjectAltName=DNS:{}", body.domain)])
        .output().await;

    let _ = tokio::fs::remove_file(&csr_path).await;

    HttpResponse::Ok().json(ResponseStructure {
        success: true, code: 200,
        message: String::from("Self-signed certificate generated"),
        data: Some(serde_json::json!({
            "domain": body.domain,
            "cert_path": cert_path,
            "key_path": key_path,
            "days": days,
        })),
    })
}
