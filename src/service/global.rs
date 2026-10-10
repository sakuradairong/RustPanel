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
    pub static ref SECURITY_KEYS: Result<super::security::SecurityKeys, String> =
        super::security::SecurityKeys::from_environment();
    pub static ref USER_PASSWORD_KEY: String = SECURITY_KEYS
        .as_ref().expect("Security keys must be validated before serving requests")
        .signing.clone();
    pub static ref SESSION_KEY: Vec<u8> = SECURITY_KEYS
        .as_ref().expect("Security keys must be validated before serving requests")
        .session.clone();
    /// RP-08 master key (KEK). Missing/invalid values refuse panel startup.
    pub static ref MASTER_KEY: Result<crate::secrets::MasterKey, String> =
        crate::secrets::MasterKey::from_environment();
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
