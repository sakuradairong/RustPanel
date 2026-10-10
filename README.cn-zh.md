<!--
 * @Descripttion: 
 * @version: 
 * @Author: Wynters
 * @Date: 2024-05-21 17:01:41
 * @LastEditTime: 2024-05-29 21:31:16
 * @FilePath: \RustPanel\README.cn-zh.md
-->
# RustPanel

_语言: 中文, [English](README.md)

## 🤔 这是什么?
![image](https://github.com/WyntersN/RustPanel/assets/27792650/5080e1e5-24f5-4fdc-a85e-be45061626b5)

这是基于Rust开发的新一代服务器面板。它能够监控服务器CPU、内存、流量流入和流出，以及管理数据库、网站、Docker，支持第三方插件开发等操作

**当项目能够正常运行时将会发布一个构建版本(Linux和Win,我相信这很快)***

***正在重构前端...***

## 项目计划

|服务 |描述|
|----------------|--------------------------------|
|支持系统|`Linux、Windows、Mac`
|站点管理|`支持的HTTP 服务器(nginx、apache、caddy) 支持的开发语言 (PHP、Node、Python、Java、Go)`
|服务器管理|`MySql、PostgreSQL、MongoDB、redis`
|Docker|`这是一个关键点，极有可能在本项目后期增加K8s，以实现K8s管理的可视化操作`
|文件管理|`您可以在线编辑代码并可视化文件管理。这是一个突出的安全问题，因此我们将在传输和保存过程中对文件进行加密`
|防火墙|`端口的开放和拒绝，SSH管理`
|资产管理|`使用一个面板管理多个面板，只需将主服务器连接地址和API密钥添加到配置文件中即可轻松管理多个面板`
|开放API|`您可以使用API自行管理`
|插件|`将实现支持Rust、Go、C和C++开发的插件`
|国际化|`中文、英语`

## 安全密钥与开发进度

面板现在要求配置独立的实例密钥：`RP_SECRET_KEY`（至少 32 字节）和 `RP_SESSION_KEY`（至少 64 字节）。缺失、过短、相同或沿用曾公开的默认值都会拒绝启动。YAML 中的 `app.secret_key` 不是 JWT 密钥来源。

参见[密钥配置及升级说明](docs/security-bootstrap.md)、[开发方案](docs/development-plan.md)和[实施进度](docs/development-progress.md)。首批安全修复已实施，其余生产准入项仍待开发和验收。
