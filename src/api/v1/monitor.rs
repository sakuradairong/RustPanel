/*
 * @Descripttion:
 * @version:
 * @Author: Wynters
 * @Date: 2024-05-25 16:23:02
 * @LastEditTime: 2024-05-25 16:23:02
 * @FilePath: \RustPanel\src\api\v1\monitor.rs
 */
use actix_web::{web, HttpRequest, HttpResponse, ResponseError};
use futures_util::stream::Stream;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use std::sync::Mutex;
use std::task::{Context, Poll};
use std::time::Duration;
use sysinfo::{Disks, Networks, System};

use crate::api::auth::{authenticate_token, AuthUser};
use crate::api::{ResponseStructure, ResponseStructureError};
use crate::errors::common::CommonError;
use crate::service::db::DBPool;
use crate::service::sse_tickets::{self, MONITOR_STREAM};

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
struct SystemStats {
    cpu: f32,
    memory: MemoryStats,
    swap: SwapStats,
    network: Vec<NetworkStats>,
    disks: Vec<DiskStats>,
}

struct MonitorStream {
    interval: tokio::time::Interval,
    pool: DBPool,
    user_id: i32,
    ticks: u32,
    closed: bool,
}

impl Stream for MonitorStream {
    type Item = Result<web::Bytes, actix_web::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.closed {
            return Poll::Ready(None);
        }
        if self.interval.poll_tick(cx).is_ready() {
            self.ticks = self.ticks.wrapping_add(1);
            // Re-check account status every ~10s (5 * 2s ticks).
            if self.ticks % 5 == 0 {
                let pool = self.pool.clone();
                let user_id = self.user_id;
                let still_ok = match pool.get() {
                    Ok(mut conn) => {
                        sse_tickets::user_still_active(&mut conn, user_id).unwrap_or(false)
                    }
                    Err(_) => false,
                };
                if !still_ok {
                    self.closed = true;
                    let bye = "event: revoked\ndata: {\"message\":\"session revoked\"}\n\n";
                    return Poll::Ready(Some(Ok(web::Bytes::from(bye))));
                }
            }
            let stats = collect_stats();
            let json = serde_json::to_string(&stats).unwrap_or_else(|_| String::new());
            let data = format!("data: {}\n\n", json);
            Poll::Ready(Some(Ok(web::Bytes::from(data))))
        } else {
            Poll::Pending
        }
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
        network,
        disks,
    }
}

fn monitor_stream(pool: DBPool, user_id: i32) -> MonitorStream {
    MonitorStream {
        interval: tokio::time::interval(Duration::from_secs(2)),
        pool,
        user_id,
        ticks: 0,
        closed: false,
    }
}

fn sse_ok(pool: DBPool, user_id: i32) -> HttpResponse {
    HttpResponse::Ok()
        .insert_header(("Content-Type", "text/event-stream"))
        .insert_header(("Cache-Control", "no-cache"))
        .insert_header(("Connection", "keep-alive"))
        .streaming(monitor_stream(pool, user_id))
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
    let user_id = user.id;
    match web::block(move || {
        let mut conn = pool.get().map_err(|_| {
            CommonError::ServiceUnavailable("SSE ticket store unavailable".into())
        })?;
        sse_tickets::issue_ticket(&mut conn, user_id, &stream, sse_tickets::DEFAULT_TTL_SECS)
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
    if params
        .as_ref()
        .and_then(|p| p.get("token"))
        .is_some()
    {
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
            let user_id = sse_tickets::consume_ticket(&mut conn, &ticket, &expected)?;
            if !sse_tickets::user_still_active(&mut conn, user_id)? {
                return Err(CommonError::Unauthorized("Invalid credentials".into()));
            }
            Ok(user_id)
        })
        .await
        {
            Ok(Ok(user_id)) => sse_ok(pool.get_ref().clone(), user_id),
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
        .map(|s| {
            s.strip_prefix("Bearer ")
                .unwrap_or(s)
                .to_string()
        });

    if let Some(token) = bearer {
        let pool_auth = pool.clone();
        return match web::block(move || authenticate_token(&token, pool_auth.get_ref())).await {
            Ok(Ok(user)) => sse_ok(pool.get_ref().clone(), user.id),
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
