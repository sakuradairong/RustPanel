use super::SiteError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SslMode {
    None,
    LetsEncrypt,
    Custom,
}

impl SslMode {
    pub fn parse(value: &str) -> Self {
        match value {
            "letsencrypt" => Self::LetsEncrypt,
            "custom" => Self::Custom,
            _ => Self::None,
        }
    }

    pub fn is_tls(self) -> bool {
        !matches!(self, Self::None)
    }
}

#[derive(Debug, Clone)]
pub struct SiteSpec {
    pub server_name: String,
    pub root: Option<String>,
    pub listen: u16,
    pub proxy_pass: Option<String>,
    pub ssl: SslMode,
    pub index: String,
    pub extra_config: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedSite {
    pub server_name: Option<String>,
    pub listen: Option<String>,
    pub proxy_pass: Option<String>,
    pub root: Option<String>,
    pub ssl: bool,
}

pub fn file_stem(server_name: &str) -> Result<String, SiteError> {
    validate_label(server_name)?;
    Ok(server_name.replace('.', "-"))
}

pub fn validate_label(label: &str) -> Result<(), SiteError> {
    let ok = !label.is_empty()
        && label.len() <= 128
        && !label.contains("..")
        && label
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
        && label
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-');
    if ok {
        Ok(())
    } else {
        Err(SiteError::Invalid(
            "name may contain only letters, numbers, dots, underscores, and hyphens".into(),
        ))
    }
}

pub fn normalize_site_stem(name: &str) -> Result<String, SiteError> {
    let stem = name
        .strip_suffix(".conf")
        .or_else(|| name.strip_suffix(".caddy.disabled"))
        .or_else(|| name.strip_suffix(".caddy"))
        .unwrap_or(name);
    validate_label(stem)?;
    Ok(stem.to_string())
}

pub fn prepare_spec(spec: &SiteSpec) -> Result<(), SiteError> {
    validate_label(&spec.server_name)?;
    if spec.listen == 0 {
        return Err(SiteError::Invalid(
            "listen port must be between 1 and 65535".into(),
        ));
    }
    match (&spec.root, &spec.proxy_pass) {
        (None, None) => {
            return Err(SiteError::Invalid(
                "root directory or proxy pass is required".into(),
            ));
        }
        (Some(root), _) => validate_root(root)?,
        _ => {}
    }
    if let Some(proxy) = &spec.proxy_pass {
        validate_proxy(proxy)?;
    }
    validate_plain(&spec.index, "index")?;
    Ok(())
}

pub fn render_apache(spec: &SiteSpec) -> Result<String, SiteError> {
    prepare_spec(spec)?;
    let mut config = format!("<VirtualHost *:{}>\n", spec.listen);
    config.push_str(&format!("    ServerName {}\n", spec.server_name));

    if let Some((cert, key)) = cert_paths(spec) {
        config.push_str("    SSLEngine on\n");
        config.push_str(&format!("    SSLCertificateFile {cert}\n"));
        config.push_str(&format!("    SSLCertificateKeyFile {key}\n"));
    }

    if let Some(proxy) = &spec.proxy_pass {
        config.push_str("    ProxyPreserveHost On\n");
        config.push_str(&format!("    ProxyPass / {proxy}\n"));
        config.push_str(&format!("    ProxyPassReverse / {proxy}\n"));
    } else if let Some(root) = &spec.root {
        config.push_str(&format!("    DocumentRoot {root}\n"));
        config.push_str(&format!("    <Directory \"{root}\">\n"));
        config.push_str("        Options FollowSymLinks\n");
        config.push_str("        AllowOverride All\n");
        config.push_str("        Require all granted\n");
        config.push_str(&format!("        DirectoryIndex {}\n", spec.index));
        config.push_str("    </Directory>\n");
    }

    push_extra(&mut config, &spec.extra_config);
    config.push_str("</VirtualHost>\n");
    Ok(config)
}

pub fn render_caddy(spec: &SiteSpec) -> Result<String, SiteError> {
    prepare_spec(spec)?;
    let mut config = format!("{} {{\n", caddy_address(spec));
    if spec.ssl == SslMode::Custom {
        if let Some((cert, key)) = cert_paths(spec) {
            config.push_str(&format!("    tls {cert} {key}\n"));
        }
    }
    if let Some(proxy) = &spec.proxy_pass {
        config.push_str(&format!("    reverse_proxy {proxy}\n"));
    } else if let Some(root) = &spec.root {
        config.push_str(&format!("    root * {root}\n"));
        config.push_str("    encode gzip\n");
        config.push_str(&format!(
            "    file_server {{\n        index {}\n    }}\n",
            spec.index
        ));
    }
    push_extra(&mut config, &spec.extra_config);
    config.push_str("}\n");
    Ok(config)
}

pub fn parse_apache(content: &str) -> ParsedSite {
    let mut parsed = ParsedSite {
        server_name: None,
        listen: None,
        proxy_pass: None,
        root: None,
        ssl: false,
    };
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            continue;
        }
        if parsed.server_name.is_none() {
            if let Some(value) = trimmed.strip_prefix("ServerName") {
                parsed.server_name = first_token(value);
            }
        }
        if parsed.root.is_none() {
            if let Some(value) = trimmed.strip_prefix("DocumentRoot") {
                parsed.root = first_token(value).map(|value| value.trim_matches('"').to_string());
            }
        }
        if parsed.proxy_pass.is_none() && trimmed.starts_with("ProxyPass ") {
            let mut parts = trimmed.split_whitespace();
            let _directive = parts.next();
            let _path = parts.next();
            parsed.proxy_pass = parts.next().map(str::to_string);
        }
        if parsed.listen.is_none() {
            if let Some(value) = trimmed.strip_prefix("<VirtualHost") {
                parsed.listen = value
                    .split(':')
                    .nth(1)
                    .and_then(|port| port.split(|c: char| !c.is_ascii_digit()).next())
                    .filter(|port| !port.is_empty())
                    .map(str::to_string);
            }
        }
        if trimmed.contains("SSLEngine on") || trimmed.contains(":443") {
            parsed.ssl = true;
        }
    }
    parsed
}

pub fn parse_caddy(content: &str) -> ParsedSite {
    let mut parsed = ParsedSite {
        server_name: None,
        listen: None,
        proxy_pass: None,
        root: None,
        ssl: false,
    };
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if parsed.server_name.is_none() && trimmed.ends_with('{') {
            let address = trimmed.trim_end_matches('{').trim();
            parsed.ssl = !address.starts_with("http://");
            let bare = address
                .trim_start_matches("https://")
                .trim_start_matches("http://");
            let (host, port) = bare
                .rsplit_once(':')
                .filter(|(_, port)| port.chars().all(|c| c.is_ascii_digit()))
                .map(|(host, port)| (host.to_string(), Some(port.to_string())))
                .unwrap_or((bare.to_string(), None));
            parsed.server_name = Some(host);
            parsed.listen =
                port.or_else(|| Some(if parsed.ssl { "443" } else { "80" }.to_string()));
            continue;
        }
        if parsed.root.is_none() {
            if let Some(value) = trimmed.strip_prefix("root *") {
                parsed.root = first_token(value);
            }
        }
        if parsed.proxy_pass.is_none() {
            if let Some(value) = trimmed.strip_prefix("reverse_proxy") {
                parsed.proxy_pass = first_token(value);
            }
        }
        if trimmed.starts_with("tls ") {
            parsed.ssl = true;
        }
    }
    parsed
}

fn cert_paths(spec: &SiteSpec) -> Option<(String, String)> {
    let stem = spec.server_name.replace('.', "-");
    match spec.ssl {
        SslMode::LetsEncrypt => Some((
            format!("/etc/letsencrypt/live/{}/fullchain.pem", spec.server_name),
            format!("/etc/letsencrypt/live/{}/privkey.pem", spec.server_name),
        )),
        SslMode::Custom => Some((
            format!("/etc/ssl/rustpanel/{stem}.crt"),
            format!("/etc/ssl/rustpanel/{stem}.key"),
        )),
        SslMode::None => None,
    }
}

fn caddy_address(spec: &SiteSpec) -> String {
    if spec.ssl == SslMode::None {
        if spec.listen == 80 {
            format!("http://{}", spec.server_name)
        } else {
            format!("http://{}:{}", spec.server_name, spec.listen)
        }
    } else if spec.listen == 80 || spec.listen == 443 {
        spec.server_name.clone()
    } else {
        format!("{}:{}", spec.server_name, spec.listen)
    }
}

fn push_extra(config: &mut String, extra: &Option<String>) {
    if let Some(extra) = extra {
        let extra = extra.trim();
        if !extra.is_empty() {
            config.push_str(extra);
            if !extra.ends_with('\n') {
                config.push('\n');
            }
        }
    }
}

fn validate_root(root: &str) -> Result<(), SiteError> {
    validate_plain(root, "root")?;
    if root.starts_with('/') {
        Ok(())
    } else {
        Err(SiteError::Invalid("root must be an absolute path".into()))
    }
}

fn validate_proxy(proxy: &str) -> Result<(), SiteError> {
    validate_plain(proxy, "proxy pass")?;
    if proxy.starts_with("http://") || proxy.starts_with("https://") {
        Ok(())
    } else {
        Err(SiteError::Invalid(
            "proxy pass must start with http:// or https://".into(),
        ))
    }
}

fn validate_plain(value: &str, field: &str) -> Result<(), SiteError> {
    let ok = !value.is_empty()
        && !value
            .chars()
            .any(|c| c.is_control() || matches!(c, '"' | '\'' | '<' | '>' | '{' | '}' | '\\'));
    if ok {
        Ok(())
    } else {
        Err(SiteError::Invalid(format!(
            "{field} contains unsupported characters"
        )))
    }
}

fn first_token(value: &str) -> Option<String> {
    let token = value.trim().split_whitespace().next()?.trim_matches('"');
    if token.is_empty() {
        None
    } else {
        Some(token.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(ssl: SslMode, proxy: Option<&str>, root: Option<&str>) -> SiteSpec {
        SiteSpec {
            server_name: "example.com".into(),
            root: root.map(str::to_string),
            listen: if ssl.is_tls() { 443 } else { 80 },
            proxy_pass: proxy.map(str::to_string),
            ssl,
            index: "index.html index.htm".into(),
            extra_config: None,
        }
    }

    #[test]
    fn rejects_path_traversal_in_site_name() {
        assert!(file_stem("../etc").is_err());
        assert!(file_stem("example.com/../../x").is_err());
        assert!(normalize_site_stem("..conf").is_err());
    }

    #[test]
    fn apache_static_site_contains_document_root() {
        let config = render_apache(&sample(SslMode::None, None, Some("/var/www/example"))).unwrap();
        assert!(config.contains("ServerName example.com"));
        assert!(config.contains("DocumentRoot /var/www/example"));
        assert!(config.contains("<VirtualHost *:80>"));
        let parsed = parse_apache(&config);
        assert_eq!(parsed.server_name.as_deref(), Some("example.com"));
        assert_eq!(parsed.root.as_deref(), Some("/var/www/example"));
        assert!(!parsed.ssl);
    }

    #[test]
    fn apache_proxy_and_certificate_sites() {
        let proxy =
            render_apache(&sample(SslMode::None, Some("http://127.0.0.1:3000"), None)).unwrap();
        assert!(proxy.contains("ProxyPass / http://127.0.0.1:3000"));
        assert!(!proxy.contains("DocumentRoot"));

        let tls = render_apache(&sample(
            SslMode::LetsEncrypt,
            None,
            Some("/var/www/example"),
        ))
        .unwrap();
        assert!(tls.contains("SSLCertificateFile /etc/letsencrypt/live/example.com/fullchain.pem"));
        let parsed = parse_apache(&tls);
        assert!(parsed.ssl);
        assert_eq!(parsed.listen.as_deref(), Some("443"));
    }

    #[test]
    fn caddy_plain_site_disables_automatic_https() {
        let config = render_caddy(&sample(SslMode::None, None, Some("/var/www/example"))).unwrap();
        assert!(config.starts_with("http://example.com {"));
        assert!(config.contains("root * /var/www/example"));
        let parsed = parse_caddy(&config);
        assert_eq!(parsed.server_name.as_deref(), Some("example.com"));
        assert!(!parsed.ssl);
        assert_eq!(parsed.root.as_deref(), Some("/var/www/example"));
    }

    #[test]
    fn caddy_tls_modes() {
        let auto = render_caddy(&sample(
            SslMode::LetsEncrypt,
            Some("https://127.0.0.1:3000"),
            None,
        ))
        .unwrap();
        assert!(auto.starts_with("example.com {"));
        assert!(auto.contains("reverse_proxy https://127.0.0.1:3000"));
        assert!(!auto.contains("\n    tls "));

        let custom = render_caddy(&sample(SslMode::Custom, None, Some("/srv/www"))).unwrap();
        assert!(custom
            .contains("tls /etc/ssl/rustpanel/example-com.crt /etc/ssl/rustpanel/example-com.key"));
        assert!(parse_caddy(&custom).ssl);
    }

    #[test]
    fn rejects_relative_root_and_newline() {
        assert!(render_apache(&sample(SslMode::None, None, Some("www"))).is_err());
        assert!(render_caddy(&sample(
            SslMode::None,
            Some("http://127.0.0.1:3000\nimport"),
            None
        ))
        .is_err());
    }
}
