# 开发环境快速配置助手

一个面向 Windows 开发者的一站式环境组装工具：选好要装的软件，下载、安装引导、环境变量、镜像配置全部由工具完成。

基于 Tauri 2 + Svelte 构建，安装包体积小、无需常驻后台。

## 它能做什么

- **一键安装开发软件**：Node.js、Python、Rust、Go、.NET、Java、Git、CMake、MariaDB、MySQL、Redis、MongoDB、nginx、Apache、VC++ 运行库、Visual Studio 等二十余款，另有 Trae、Cursor、CodeBuddy 等 AI 编程工具
- **版本自主选择**：从官方渠道拉取版本列表，默认选中最新稳定版，支持「仅看 LTS」过滤
- **国内镜像回落**：下载优先官网，失败自动回落国内镜像；镜像地址全部经过真实可用性验证，不会内置失效地址
- **下载缓存**：同一版本的安装包只下载一次，下载管理支持查看、删除与清空
- **环境变量安全写入**：写前自动备份、路径规范化去重（大小写、尾斜杠、变量引用）、写失败自动回滚、支持一键还原
- **包管理器换源**：npm、pnpm、Yarn、pip、Conda、Cargo、Go Modules、NuGet、RubyGems、Composer、Maven、Gradle、CRAN、Bun、rustup 一键换源，重复执行不会写坏配置
- **安装状态真实检测**：以注册表卸载项为准，自定义安装盘符与目录也能正确识别
- **AI 编程 CLI**：Claude Code CLI、Codex CLI 的一键安装入口

## 下载缓存与数据位置

| 内容 | 位置 |
| --- | --- |
| 安装包缓存 | `%LOCALAPPDATA%\DevEnv\cache` |
| 环境变量备份 | `%LOCALAPPDATA%\DevEnv\env-backup` |

运行日志只保存在本机，工具不会上传任何数据。

## 从源码构建

依赖：Node.js 20+、Rust 1.77+（MSVC 工具链）、Windows 10/11。

```bash
npm install
npm run tauri dev      # 开发调试
npm run tauri build    # 产出安装包
```

## 代码结构

```
├── src/                  # 前端（Svelte 5 + TypeScript）
│   ├── views/            # 页面：软件总览、环境配置、下载管理、关于
│   └── lib/              # 组件与状态管理
└── src-tauri/            # 后端（Rust）
    ├── resources/manifest.toml   # 软件清单：所有条目、镜像与配置动作都在这里声明
    ├── src/provider/     # 版本源适配器：官方 API、目录列表、页面正则等十余种
    ├── src/config.rs     # 配置动作引擎：环境变量 / TOML / XML / 文件 / 命令 / SSH 密钥
    └── src/…             # 下载、安装、检测、环境变量、路径规范化等模块
```

新增软件或换源项只需修改 `manifest.toml`，必要时在 `provider/` 下补一个版本源适配器，无需改动界面代码。

## 参与贡献

欢迎提 Issue 与 Pull Request：

- 软件清单的地址失效、新软件支持请求
- 镜像源的可用性反馈（我们依赖真实用户的验证来维护清单）
- 界面与体验改进

## 协议

本项目基于 [MIT License](LICENSE) 开源。

## 作者

小念 —— 项目的开发者与维护者。
