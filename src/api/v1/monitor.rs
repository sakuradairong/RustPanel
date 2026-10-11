/*
 * @Descripttion:
 * @version:
 * @Author: Wynters
 * @Date: 2024-05-25 16:23:02
 * @LastEditTime: 2024-05-25 16:23:02
 * @FilePath: \RustPanel\src\api\v1\monitor.rs
 */
use actix_web::{body::BodyStream, web, HttpRequest, HttpResponse, ResponseError};
use futures_util::stream::Stream;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::time::Duration;
use sysinfo::{Disks, Networks, System};

use crate::api::auth::{authenticate_token, AuthUser};
use crate::api::{ResponseStructure, ResponseStructureError};
use crate::errors::common::CommonError;
use crate::service::db::DBPool;
use crate::service::sse_tickets::{self, MONITOR_STREAM, StreamIdentity};

lazy_static! {
    static ref SYS: Mutex<System> = Mutex::new(System::new_all());
}

#[derive(Serialize)]
struct MemoryStats {
    used: u64,
    total: u64,
}

#[derive(Serialize)]
struct SwapStats {
    used: u64,
    total: u64,
}

#[derive(Serialize)]
struct NetworkStats {
    name: String,
    received: u64,
    transmitted: u64,
}

#[derive(Serialize)]
struct DiskStats {
    name: String,
    read_bytes: u64,
    written_bytes: u64,
}

#[derive(Serialize)]
struct LoadStats {
    one: f64,
    five: f64,
    fifteen: f64,
}

#[derive(Serialize)]
struct SystemStats {
    cpu: f32,
    memory: MemoryStats,
    swap: SwapStats,
    load: LoadStats,
    network: Vec<NetworkStats>,
    disks: Vec<DiskStats>,
}

/// Feeds the SSE body from a spawned task so no blocking work (SQLite status
/// checks, sysinfo refresh) ever runs on the async worker thread.
async fn run_monitor_stream(
    pool: DBPool,
    identity: StreamIdentity,
    tx: tokio::sync::mpsc::Sender<web::Bytes>,
) {
    let mut interval = tokio::time::interval(Duration::from_secs(2));
    loop {
        interval.tick().await;
        // Re-check the issuing session and current permissions every 2s. DB errors and
        // join failures fail closed (stream ends).
        {
            let check_pool = pool.clone();
            let check_identity = identity.clone();
            let still_ok = tokio::task::spawn_blocking(move || match check_pool.get() {
                Ok(mut conn) => sse_tickets::validate_identity(&mut conn, &check_identity).is_ok(),
                Err(_) => false,
            })
            .await
            .unwrap_or(false);
            if !still_ok {
                let bye = "event: revoked\ndata: {\"message\":\"session revoked\"}\n\n";
                let _ = tx.send(web::Bytes::from(bye)).await;
                return;
            }
        }
        let stats = tokio::task::spawn_blocking(collect_stats).await;
        let json = match stats {
            Ok(stats) => serde_json::to_string(&stats).unwrap_or_else(|_| String::new()),
            Err(_) => return,
        };
        let data = format!("data: {json}\n\n");
        if tx.send(web::Bytes::from(data)).await.is_err() {
            // Client disconnected; stop producing.
            return;
        }
    }
}

struct MonitorChannelStream(tokio::sync::mpsc::Receiver<web::Bytes>);

impl Stream for MonitorChannelStream {
    type Item = Result<web::Bytes, actix_web::Error>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.0.poll_recv(cx).map(|item| item.map(Ok))
    }
}

fn collect_stats() -> SystemStats {
    let mut sys = SYS.lock().unwrap();
    sys.refresh_cpu_all();
    sys.refresh_memory();

    let cpu = sys.global_cpu_usage();

    let memory = MemoryStats {
        used: sys.used_memory(),
        total: sys.total_memory(),
    };

    let swap = SwapStats {
        used: sys.used_swap(),
        total: sys.total_swap(),
    };

    let la = System::load_average();
    let load = LoadStats {
        one: la.one,
        five: la.five,
        fifteen: la.fifteen,
    };

    let network = Networks::new_with_refreshed_list()
        .into_iter()
        .map(|(name, data)| NetworkStats {
            name: name.clone(),
            received: data.received(),
            transmitted: data.transmitted(),
        })
        .collect();

    let disks = Disks::new_with_refreshed_list()
        .iter()
        .map(|disk| DiskStats {
            name: disk.name().to_string_lossy().to_string(),
            read_bytes: disk.usage().total_read_bytes,
            written_bytes: disk.usage().total_written_bytes,
        })
        .collect();

    SystemStats {
        cpu,
        memory,
        swap,
        load,
        network,
        disks,
    }
}

fn sse_ok(pool: DBPool, identity: StreamIdentity) -> HttpResponse {
    let (tx, rx) = tokio::sync::mpsc::channel::<web::Bytes>(8);
    tokio::spawn(run_monitor_stream(pool, identity, tx));
    HttpResponse::Ok()
        .insert_header(("Content-Type", "text/event-stream"))
        .insert_header(("Cache-Control", "no-cache"))
        .insert_header(("Connection", "keep-alive"))
        // Prevent the Compress middleware from buffering/compressing the
        // stream: SSE must be flushed per-event, and brotli/gzip buffers it.
        .insert_header(("Content-Encoding", "identity"))
        .body(BodyStream::new(MonitorChannelStream(rx)))
}

#[derive(Deserialize)]
pub struct TicketBody {
    #[serde(default = "default_stream")]
    pub stream: String,
}

fn default_stream() -> String {
    MONITOR_STREAM.to_string()
}

/// Issue a ≤60s one-time SSE ticket for EventSource clients.
pub async fn issue_sse_ticket(
    user: AuthUser,
    pool: web::Data<DBPool>,
    body: web::Json<TicketBody>,
) -> HttpResponse {
    let stream = body.stream.clone();
    let identity = StreamIdentity::from(&user);
    match web::block(move || {
        let mut conn = pool
            .get()
            .map_err(|_| CommonError::ServiceUnavailable("SSE ticket store unavailable".into()))?;
        sse_tickets::issue_ticket(&mut conn, &identity, &stream, sse_tickets::DEFAULT_TTL_SECS)
    })
    .await
    {
        Ok(Ok((ticket, expires_in))) => HttpResponse::Ok().json(ResponseStructure {
            success: true,
            code: 200,
            message: String::from("success"),
            data: Some(serde_json::json!({
                "ticket": ticket,
                "stream": body.stream,
                "expires_in": expires_in,
            })),
        }),
        Ok(Err(err)) => err.error_response(),
        Err(_) => CommonError::ServiceUnavailable("SSE ticket service unavailable".into())
            .error_response(),
    }
}

pub async fn monitor_sse(req: HttpRequest) -> HttpResponse {
    let Some(pool) = req.app_data::<web::Data<DBPool>>().cloned() else {
        return CommonError::ServiceUnavailable("Authentication database unavailable".into())
            .error_response();
    };

    let params =
        web::Query::<std::collections::HashMap<String, String>>::from_query(req.query_string())
            .ok();

    // Long-lived JWTs must not appear in URLs (logs / Referer).
    if params.as_ref().and_then(|p| p.get("token")).is_some() {
        return HttpResponse::Unauthorized().json(ResponseStructureError {
            success: false,
            code: 401,
            message: String::from(
                "Long-lived tokens in query strings are not allowed; request an SSE ticket",
            ),
        });
    }

    if let Some(ticket) = params.as_ref().and_then(|p| p.get("ticket").cloned()) {
        let pool_ticket = pool.clone();
        let expected = MONITOR_STREAM.to_string();
        return match web::block(move || {
            let mut conn = pool_ticket.get().map_err(|_| {
                CommonError::ServiceUnavailable("SSE ticket store unavailable".into())
            })?;
            sse_tickets::consume_ticket(&mut conn, &ticket, &expected)
        })
        .await
        {
            Ok(Ok(identity)) => sse_ok(pool.get_ref().clone(), identity),
            Ok(Err(err)) => err.error_response(),
            Err(_) => CommonError::ServiceUnavailable("SSE ticket service unavailable".into())
                .error_response(),
        };
    }

    // Non-EventSource clients may still use Authorization: Bearer (not query).
    let bearer = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.strip_prefix("Bearer ").unwrap_or(s).to_string());

    if let Some(token) = bearer {
        let pool_auth = pool.clone();
        return match web::block(move || {
            let user = authenticate_token(&token, pool_auth.get_ref())?;
            if user.must_change_password {
                return Err(CommonError::Forbidden(
                    "Password change required before continuing".into(),
                ));
            }
            let mut conn = pool_auth.get().map_err(|_| {
                CommonError::ServiceUnavailable("Authentication database unavailable".into())
            })?;
            sse_tickets::validate_identity(&mut conn, &StreamIdentity::from(&user))?;
            Ok::<_, CommonError>(user)
        })
        .await
        {
            Ok(Ok(user)) => sse_ok(pool.get_ref().clone(), StreamIdentity::from(&user)),
            Ok(Err(error)) => error.error_response(),
            Err(_) => CommonError::ServiceUnavailable("Authentication service unavailable".into())
                .error_response(),
        };
    }

    HttpResponse::Unauthorized().json(ResponseStructureError {
        success: false,
        code: 401,
        message: String::from("Unauthorized"),
    })
}
