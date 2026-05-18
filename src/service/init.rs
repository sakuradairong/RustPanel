/*
 * @Descripttion: System initialization logic
 * @version:
 * @Author: Wynters
 * @Date: 2024-05-08 18:12:32
 * @LastEditTime: 2025-12-15 13:17:54
 * @FilePath: \RustPanel\src\service\init.rs
 */

use std::fs;
use std::path::Path;

use crate::{log_info, service::global::CONF};

/// Initialize the application on startup.
/// Creates required directories, validates configuration, and prepares runtime state.
pub fn init_app() {
    log_info!("Initializing RustPanel...");

    // 1. Create data directory for SQLite if it doesn't exist
    if let Some(parent) = Path::new(&CONF.database.path).parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).expect("Failed to create database directory");
            log_info!("Created database directory: {:?}", parent);
        }
    }

    // 2. Create server directory for installed software
    let server_dir = Path::new("./server");
    if !server_dir.exists() {
        fs::create_dir_all(server_dir).expect("Failed to create server directory");
        log_info!("Created server directory: ./server");
    }

    // 3. Create runtime directory for temp/pid files
    let runtime_dir = Path::new("./runtime");
    if !runtime_dir.exists() {
        fs::create_dir_all(runtime_dir).expect("Failed to create runtime directory");
        log_info!("Created runtime directory: ./runtime");
    }

    // 4. Validate configuration
    if CONF.app.workers == 0 {
        let cpu_count = num_cpus::get();
        log_info!("Auto-detected {} CPU cores", cpu_count);
    }

    log_info!("Initialization complete.");
}
