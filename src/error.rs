use thiserror::Error;

/// mt5-rs 的统一错误类型。
///
/// 所有可能失败的 API 都会返回 [`Result<T>`]（即 `Result<T, Mt5Error>`），
/// 调用方可以通过 `match` 或 `?` 操作符统一处理这些错误。
///
/// 错误来源主要分为三类：
/// 1. **系统/IO 错误**（[`Mt5Error::IoError`]）：管道读写失败等 Windows 底层错误；
/// 2. **协议错误**（[`Mt5Error::ProtocolError`] / [`Mt5Error::InvalidResponse`]）：
///    报文格式不符合预期、响应数据截断或解析失败；
/// 3. **业务错误**（[`Mt5Error::ConnectionFailed`] / [`Mt5Error::CommandFailed`] /
///    [`Mt5Error::NotInitialized`] / [`Mt5Error::NotSupported`]）：连接失败、
///    MT5 返回错误状态码、未初始化就调用 API、功能尚未实现等。
#[derive(Error, Debug)]
pub enum Mt5Error {
    /// 底层 IO 错误（如管道读写失败），自动从 `std::io::Error` 转换而来。
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    /// 连接失败，例如：管道名未提供、无法打开管道（MT5 未运行或管道名错误）。
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    /// 管道响应超时：终端在约定时间内没有返回响应。
    ///
    /// 触发后该连接会被**标记为失效**（避免管道内残留响应导致后续请求串包），
    /// 后续调用直接返回错误，需要重新 [`crate::Mt5Client::initialize`]。
    #[error("Timeout: {0}")]
    Timeout(String),

    /// 协议层错误，例如：无法创建进程快照、无法获取进程路径等。
    #[error("Protocol error: {0}")]
    ProtocolError(String),

    /// 响应数据无效，例如：响应长度过短、字符串字段读取失败。
    #[error("Invalid response: {0}")]
    InvalidResponse(String),

    /// 客户端尚未初始化：在调用 `initialize` 之前就使用了其他 API。
    #[error("MT5 not initialized")]
    NotInitialized,

    /// MT5 命令执行失败：`cmd` 为命令码，`error_code` 为终端返回的错误码，
    /// `error` 为终端返回的错误描述。
    #[error("Command failed: cmd={cmd}, code={error_code}, error={error}")]
    CommandFailed { cmd: u32, error_code: i32, error: String },

    /// 功能暂不支持（如 `order_check`、`order_send` 尚未实现）。
    #[error("Not supported: {0}")]
    NotSupported(String),
}

/// 本库统一使用的结果类型别名：`Result<T>` 等价于 `std::result::Result<T, Mt5Error>`。
pub type Result<T> = std::result::Result<T, Mt5Error>;

impl Mt5Error {
    /// 提取数值错误码（供 `last_error` 本地维护使用；无法提取时返回 -1）。
    ///
    /// - [`Mt5Error::CommandFailed`]：返回终端返回的错误码（如 10030）；
    /// - [`Mt5Error::IoError`]：返回底层 Windows 错误码；
    /// - [`Mt5Error::Timeout`]：返回 `-10086`（本库自定义的超时码，便于与终端错误码区分）；
    /// - 其余变体：返回 -1。
    pub fn error_code(&self) -> i32 {
        match self {
            Mt5Error::IoError(e) => e.raw_os_error().unwrap_or(-1),
            Mt5Error::CommandFailed { error_code, .. } => *error_code,
            Mt5Error::Timeout(_) => -10086,
            _ => -1,
        }
    }

    /// 是否为「连接已失效」类错误（超时导致连接被标记失效）。
    ///
    /// 调用方可据此决定是否重新 [`crate::Mt5Client::initialize`]。
    pub fn is_connection_lost(&self) -> bool {
        matches!(self, Mt5Error::Timeout(_))
    }
}
