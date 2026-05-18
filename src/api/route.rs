/*
 * @Descripttion:
 * @version:
 * @Author: Wynters
 * @Date: 2024-05-09 19:10:36
 * @LastEditTime: 2024-05-25 16:13:34
 * @FilePath: \RustPanel\src\api\route.rs
 */
use actix_web::web;

use super::v1;

pub fn v1() -> actix_web::Scope {
    web::scope("/api").service(
        web::scope("/v1")
            //.wrap(middleware::Auth)
            .service(
                web::scope("/login")
                    .route("/sign/{security_dir}", web::post().to(v1::login::sign))
                    .route("/security_dir", web::get().to(v1::login::get_security_dir)),
            )
            .service(
                web::scope("/log")
                    .service(web::resource("/list").route(web::get().to(v1::log::list_logs)))
                    .service(web::resource("/read").route(web::get().to(v1::log::read_log))),
            )
            .service(web::resource("/file/list").route(web::get().to(v1::file::list)))
            .service(web::resource("/file/content").route(web::get().to(v1::file::content)))
            .service(web::resource("/file/save").route(web::post().to(v1::file::save)))
            .service(web::resource("/user/me").route(web::get().to(v1::user::get_me)))
            .service(web::resource("/user/menus").route(web::get().to(v1::user::get_menus)))
            .service(web::resource("/os_info").route(web::get().to(v1::os::os_info)))
            .service(
                web::scope("/process")
                    .service(
                        web::resource("/list")
                            .route(web::get().to(v1::process::list_processes)),
                    )
                    .service(
                        web::resource("/kill/{pid}")
                            .route(web::post().to(v1::process::kill_process)),
                    ),
            )
            .service(
                web::scope("/docker")
                    .service(
                        web::resource("/containers")
                            .route(web::get().to(v1::docker::list_containers))
                            .route(web::post().to(v1::docker::create_container)),
                    )
                    .service(
                        web::resource("/containers/{id}/start")
                            .route(web::post().to(v1::docker::start_container)),
                    )
                    .service(
                        web::resource("/containers/{id}/stop")
                            .route(web::post().to(v1::docker::stop_container)),
                    )
                    .service(
                        web::resource("/containers/{id}/restart")
                            .route(web::post().to(v1::docker::restart_container)),
                    )
                    .service(
                        web::resource("/containers/{id}/remove")
                            .route(web::post().to(v1::docker::remove_container)),
                    )
                    .service(
                        web::resource("/containers/{id}/pause")
                            .route(web::post().to(v1::docker::pause_container)),
                    )
                    .service(
                        web::resource("/containers/{id}/unpause")
                            .route(web::post().to(v1::docker::unpause_container)),
                    )
                    .service(
                        web::resource("/containers/{id}/logs")
                            .route(web::get().to(v1::docker::get_container_logs)),
                    )
                    .service(
                        web::resource("/networks")
                            .route(web::get().to(v1::docker::list_networks))
                            .route(web::post().to(v1::docker::create_network)),
                    )
                    .service(
                        web::resource("/networks/{id}/remove")
                            .route(web::post().to(v1::docker::remove_network)),
                    )
                    .service(
                        web::resource("/networks/{id}/connect")
                            .route(web::post().to(v1::docker::connect_network)),
                    )
                    .service(
                        web::resource("/networks/{id}/disconnect")
                            .route(web::post().to(v1::docker::disconnect_network)),
                    ),
            )
            .service(
                web::scope("/firewall")
                    .service(web::resource("/status").route(web::get().to(v1::firewall::get_firewall_status)))
                    .service(web::resource("/rules").route(web::get().to(v1::firewall::list_iptables_rules)))
                    .service(web::resource("/rules").route(web::post().to(v1::firewall::add_iptables_rule)))
                    .service(web::resource("/rules/{chain}").route(web::delete().to(v1::firewall::delete_iptables_rule)))
                    .service(web::resource("/port").route(web::post().to(v1::firewall::enable_port)))
            )
            .service(
                web::scope("/webserver")
                    .service(web::resource("/status").route(web::get().to(v1::webserver::get_webserver_status)))
                    .service(web::resource("/sites").route(web::get().to(v1::webserver::list_nginx_sites)))
                    .service(web::resource("/sites").route(web::post().to(v1::webserver::create_nginx_site)))
                    .service(web::resource("/sites/{name}").route(web::delete().to(v1::webserver::delete_nginx_site)))
                    .service(web::resource("/reload").route(web::post().to(v1::webserver::reload_nginx)))
            )
            .service(web::resource("/monitor").route(web::get().to(v1::monitor::monitor_sse)))
    )
}
