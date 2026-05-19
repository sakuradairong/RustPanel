/*
 * @Descripttion: 
 * @version: 
 * @Author: Wynters
 * @Date: 2024-05-08 19:57:53
 * @LastEditTime: 2024-06-02 17:34:36
 * @FilePath: \RustPanel\src\service\global.rs
 */
use lazy_static::*; 
use crate::service::conf::Config; 


lazy_static! {
    pub static ref USER_PASSWORD_KEY: String = std::env::var("RP_SECRET_KEY")
        .unwrap_or_else(|_| String::from("4e425cc46f42a4e01ee54a3a1e7db645e629621d"));
    pub static ref SESSION_KEY: Vec<u8> = std::env::var("RP_SESSION_KEY")
        .map(|s| s.into_bytes())
        .unwrap_or_else(|_| b"749d93ea30bf13e0b4a07d8f8fe5aa488b6c83fc886a266dd2c80590026f7b49".to_vec());
}


// pub static OS_TYPE: &str = {
//     #[cfg(target_os = "linux")] {
//         "linux"
//     }
//     #[cfg(target_os = "windows")] {
//         "windows"
//     }
//     #[cfg(target_os = "macos")] {
//         "macos"
//     }
//     #[cfg(target_os = "other")] {
//         "other"
//     }
// };

lazy_static! {
    pub static ref OS_TYPE: String =  match std::env::consts::OS {
        "linux" => String::from("linux"),
        "windows" => String::from("windows"),
        "macos" => String::from("macos"),
        _ => panic!("Unsupported operating system"),
    };
}

lazy_static! {
    pub static ref CONF: Config = Config::load_from_file();
} 

lazy_static! {
    pub static ref OS_INFO : serde_json::Value = serde_json::json!({
        "name":sysinfo::System::name(),
        "kernel_version":sysinfo::System::kernel_version(),
        "os_version":sysinfo::System::os_version(),
        "host_name":sysinfo::System::host_name(),
        "boot_time":sysinfo::System::boot_time(),
        "long_os_version":sysinfo::System::long_os_version(),
        "architecture":os_info::get().architecture(),
        "os_type":os_info::get().os_type(),
    });
} 
