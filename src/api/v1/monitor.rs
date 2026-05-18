/*
 * @Descripttion:
 * @version:
 * @Author: Wynters
 * @Date: 2024-05-25 16:23:02
 * @LastEditTime: 2024-05-25 16:23:02
 * @FilePath: \RustPanel\src\api\v1\monitor.rs
 */
use actix_web::{web, HttpRequest, HttpResponse};
use futures_util::stream::Stream;
use lazy_static::lazy_static;
use serde::Serialize;
use std::pin::Pin;
use std::sync::Mutex;
use std::task::{Context, Poll};
use std::time::Duration;
use sysinfo::{Disks, Networks, System};

use crate::api::auth::AuthUser;

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
}

impl Stream for MonitorStream {
    type Item = Result<web::Bytes, actix_web::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.interval.poll_tick(cx).is_ready() {
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

fn monitor_stream() -> MonitorStream {
    MonitorStream {
        interval: tokio::time::interval(Duration::from_secs(2)),
    }
}

pub async fn monitor_sse(_: AuthUser, _req: HttpRequest) -> HttpResponse {
    HttpResponse::Ok()
        .insert_header(("Content-Type", "text/event-stream"))
        .insert_header(("Cache-Control", "no-cache"))
        .insert_header(("Connection", "keep-alive"))
        .insert_header(("Access-Control-Allow-Origin", "*"))
        .streaming(monitor_stream())
}
