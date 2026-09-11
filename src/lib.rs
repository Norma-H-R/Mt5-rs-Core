//! # mt5-rs —— 纯 Rust 实现的 MetaTrader 5 通信库
//!
//! 本库通过 **Windows 命名管道（Named Pipe）** 与正在运行的 MetaTrader 5 终端进行
//! 进程间通信（IPC），提供与 Python 官方 `MetaTrader5` 库一致的 API 接口，
//! 无需安装 Python 或任何 Python 依赖。
//!
//! ## 模块结构
//!
//! - [`client`]：核心客户端 [`Mt5Client`]，封装了所有面向用户的 API（账户、行情、交易等）
//! - [`protocol`]：底层命名管道通信协议（请求/响应报文编解码、管道自动发现）
//! - [`types`]：与 MT5 终端交互的数据结构（账户信息、品种信息、K线、Tick、持仓、订单、成交等）
//! - [`error`]：统一的错误类型 [`Mt5Error`] 与结果别名 [`Result`]
//!
//! ## 健壮性能力
//!
//! - **管道读取模式**：连接即为消息模式（`PIPE_READMODE_MESSAGE`），避免 `WriteFile` 永久阻塞；
//! - **并发保护**：[`Mt5Client`] 实现 `Send + Sync`，内部写锁串行化请求，可多线程共享；
//! - **超时保护**：单次请求默认 3 秒超时（[`DEFAULT_TIMEOUT_MS`]，可调），超时即标记连接失效；
//! - **多终端发现**：[`discover_all_terminals`] / [`discover_all_mt5_pipes`] 枚举本机在线终端。
//!
//! 完整演示见 `examples/pipe_features.rs`（`cargo run --example pipe_features`）。
//!
//! ## 快速示例
//!
//! ```no_run
//! use mt5_rs::{Mt5Client, discover_mt5_pipe};
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // 1. 自动发现正在运行的 MT5 终端对应的命名管道
//!     let pipe_name = discover_mt5_pipe();
//!
//!     // 2. 初始化客户端并建立连接
//!     let mut client = Mt5Client::new();
//!     client.initialize(Some(&pipe_name))?;
//!
//!     // 3. 获取账户信息并打印
//!     let account = client.account_info()?;
//!     println!("余额: {}", account.balance);
//!     println!("净值: {}", account.equity);
//!     println!("可用保证金: {}", account.free_margin);
//!
//!     Ok(())
//! }
//! ```

pub mod client;
pub mod error;
pub mod protocol;
pub mod types;

pub use client::Mt5Client;
pub use error::{Mt5Error, Result};
pub use types::*;
pub use protocol::{
    compute_pipe_name, discover_all_mt5_pipes, discover_all_terminals, discover_mt5_pipe,
    find_terminal64_paths, NamedPipeClient, DEFAULT_TIMEOUT_MS,
};
