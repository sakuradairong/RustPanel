/*
 * @Descripttion:
 * @version:
 * @Author: Wynters
 * @Date: 2024-05-10 16:11:29
 * @LastEditTime: 2024-05-29 16:17:40
 * @FilePath: \RustPanel\src\api\v1\user.rs
 */

use actix_web::HttpResponse;

use crate::api::auth::AuthUser;

pub async fn get_me(user: AuthUser) -> HttpResponse {
    HttpResponse::Ok().json(user)
}

pub async fn get_menus(_: AuthUser) -> HttpResponse {
    let menus = serde_json::json!([{"id":1,"name":"首页","path":"/home","icon":"⌂","key":"home","sort":1},{"id":2,"name":"网站管理","path":"/webserver","icon":"🌐","key":"webserver","sort":2},{"id":3,"name":"文件管理","path":"/file/list","icon":"📁","key":"file.list","sort":3},{"id":4,"name":"Docker","path":"/docker/containers","icon":"🐳","key":"docker","sort":4},{"id":5,"name":"系统信息","path":"/system/info","icon":"ℹ","key":"system.info","sort":5},{"id":6,"name":"用户管理","path":"/admin/users","icon":"👥","key":"admin.users","sort":6},{"id":7,"name":"进程管理","path":"/process","icon":"⚡","key":"process","sort":7},{"id":8,"name":"服务管理","path":"/services","icon":"🔧","key":"services","sort":8},{"id":9,"name":"日志查看","path":"/logs","icon":"📋","key":"logs","sort":9},{"id":10,"name":"实时监控","path":"/monitor","icon":"📊","key":"monitor","sort":10},{"id":11,"name":"防火墙","path":"/firewall","icon":"🛡","key":"firewall","sort":11}]);

    HttpResponse::Ok().json(serde_json::json!({"data":{"info":{"id":1,"username":"admin","nickname":"Xin Admin","avatar":"https://api.iconify.design/logos:github-copilot.svg?color=#888888","sex":"0","email":"111@qq.com","mobile":"15966666666","status":"1","group_id":1,"create_time":"2022-02-26 19:55:29","update_time":"2023-10-19 14:34:00"},"access":["admin.list", "admin.group", "home", "system","file.list","web.list", "system.dict", "admin.rule", "admin.list.list", "admin.list.add", "admin.list.edit", "admin.group.list", "admin.group.add", "admin.group.edit", "admin.group.rule", "admin.group.ruleEdit", "system.dict.list", "data.descriptions", "system.dict.add", "admin.rule.list", "admin.rule.add", "admin.rule.edit", "system.dict.item.list", "system.dict.item.add", "system.dict.item.edit", "data.list", "data.checkcard", "admin.group.delete", "system.dict.delete", "system.dict.edit", "system.dict.item.delete", "admin.list.delete", "admin.rule.delete", "user.list", "online.table", "user", "system.info", "online", "xinadmin", "admin", "data", "system.setting", "system.setting.querySettingGroup", "system.setting.addGroup", "system.setting.querySettingPid", "online.table.list", "online.table.edit", "online.table.delete", "online.table.devise", "online.table.saveData", "online.table.getData", "online.table.crud", "system.setting.list", "system.setting.add", "system.setting.edit", "system.setting.delete", "content.article.add", "content.article.edit", "content.article.delete", "content.article.list", "user.rule", "user.group", "user.list", "user.edit", "user.add", "user.delete", "user.group.add", "user.group.list", "user.group.edit", "user.group.delete", "user.group.rule", "user.group.ruleEdit", "user.money_log", "user.money_log.list", "user.money_log.add", "online.table.add", "user.money_log.delete", "data.form", "admin.list.updatePwd"],"menus":menus,"success":true,"status":200,"msg":"ok","showType":0}}))
}
