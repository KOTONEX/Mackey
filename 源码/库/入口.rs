// SPDX-License-Identifier: AGPL-3.0-or-later
#[path = "变更日志.rs"]
pub mod 变更日志;
#[path = "开发.rs"]
pub mod 开发;
#[path = "引擎下载.rs"]
pub mod 引擎下载;
#[path = "打包.rs"]
pub mod 打包;
#[path = "服务.rs"]
pub mod 服务;
#[path = "焦点桥.rs"]
pub mod 焦点桥;
#[path = "用户配置.rs"]
pub mod 用户配置;
#[path = "端到端.rs"]
pub mod 端到端;
#[path = "路径.rs"]
pub mod 路径;
#[path = "配置生成.rs"]
pub mod 配置生成;
#[path = "键盘识别.rs"]
pub mod 键盘识别;
#[path = "验证.rs"]
pub mod 验证;

pub const 扩展标识: &str = "mackey-focus@kotonex";
pub const 旧扩展标识: &str = "xremap-compat@mackey.local";
pub const 嵌入清单: &str = include_str!("../../配置/行为清单.json");
pub const 版本: &str = env!("CARGO_PKG_VERSION");
/// 版本由 Cargo 自动写入；安装和发布共用同一份元数据。
pub const 扩展元数据: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/metadata.json"));

/// TypeScript 构建产物；运行、安装和发布共用同一份字节。
pub const 扩展脚本: &[u8] = include_bytes!(concat!(
    env!("OUT_DIR"),
    "/类型脚本/扩展/mackey-focus@kotonex/扩展.js"
));
pub const 扩展契约脚本: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/类型脚本/测试/扩展契约.js"));
pub const 基类桩脚本: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/类型脚本/测试/基类桩.js"));
