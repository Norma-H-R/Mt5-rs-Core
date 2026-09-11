//! 命名管道（Named Pipe）通信协议层。
//!
//! MT5 终端会以 `\\.\pipe\MT5.Terminal.<HASH>` 的名称暴露一个命名管道，
//! 本模块负责：连接管道、按协议编码请求/解码响应、自动发现管道名称。
//!
//! ## 报文格式（与 Python `MetaTrader5` 库 / `go-mt5` 一致）
//!
//! **请求帧**（客户端 → 终端）：
//!
//! ```text
//! +----------------+----------------+------------------+
//! | 总长度 (u32 LE) | 命令码 (u32 LE) | 命令参数 (原始字节) |
//! +----------------+----------------+------------------+
//! ```
//!
//! 其中「总长度」= 4（命令码）+ 参数长度，小端序（LE）。
//!
//! **响应帧**（终端 → 客户端）：
//!
//! ```text
//! +----------------+----------------+------------------+------------------+
//! | 载荷长度 (u32 LE) | 命令码 (u32 LE) | 成功标志 (u32 LE) | 返回数据 (原始字节) |
//! +----------------+----------------+------------------+------------------+
//! ```
//!
//! 载荷长度不足 8 字节视为非法响应；返回数据为载荷去掉 8 字节头后的部分。
//!
//! ## 字符串编码
//!
//! 字符串参数一律按 **UTF-16LE** 编码：先写 4 字节字符数（u32 LE），再写各字符。
//!
//! ## 本层提供的四项健壮性保障（详见 `examples/pipe_features.rs`）
//!
//! 1. **管道读取模式**：连接后立刻 `SetNamedPipeHandleState(PIPE_READMODE_MESSAGE)`。
//!    MT5 终端管道是*消息模式*管道，客户端若不显式声明按消息读取，
//!    `WriteFile` 会永久阻塞（字节模式写消息管道）——Python `MetaTrader5`
//!    与 `go-mt5` 均做此设置，必须保留。
//! 2. **并发保护**：内部 `write_lock`（`Arc<Mutex<()>>`）把「写请求 + 读响应」
//!    整段串行化。同一管道句柄上**不允许**并发 `ReadFile`/`WriteFile`
//!    （会永久阻塞、请求与响应错配），因此 `NamedPipeClient` 实现了 `Sync`，
//!    可被多线程安全共享（见 `unsafe impl Sync` 的安全说明）。
//! 3. **超时保护**：每次请求在独立线程执行「写 + 读」，调用方 `recv_timeout`
//!    等待 [`DEFAULT_TIMEOUT_MS`]（可调）。超时后**把连接标记为失效**
//!    （`broken`），后续请求立即报错——避免管道中残留响应被下一个请求读到
//!    （串包），宁可用「重新 initialize」换取确定性。
//! 4. **多终端发现**：[`discover_all_terminals`] / [`discover_all_mt5_pipes`]
//!    枚举本机全部在线 MT5 终端（可拿到 `terminal64.exe` 路径），
//!    供多账户/多实例场景逐端连接。

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, ReadFile, WriteFile, FILE_ATTRIBUTE_NORMAL,
    OPEN_EXISTING,
};
use windows_sys::Win32::System::Pipes::{
    SetNamedPipeHandleState, WaitNamedPipeW, PIPE_READMODE_MESSAGE,
};
use windows_sys::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS, PROCESSENTRY32W,
};
use sha2::{Sha256, Digest};

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use crate::error::{Mt5Error, Result};

/// `OpenProcess` 所需的最小访问权限：仅查询进程信息（限制级别），权限要求最低。
const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;

/// 默认的响应等待超时（毫秒）。
///
/// 正常查询在同一台机器上是亚毫秒~毫秒级；3 秒只用来兜住
/// 「终端假死 / 正在重启 / 管道半连接」这类异常，避免调用线程被永久卡住。
pub const DEFAULT_TIMEOUT_MS: u64 = 3000;

/// MT5 命名管道客户端。
///
/// 封装一个已打开的管道句柄，提供「发送命令并读取响应」的完整收发能力：
///
/// - **读取模式**：构造时即设置为消息模式（`PIPE_READMODE_MESSAGE`）；
/// - **并发安全**：内部写锁串行化「写 + 读」，类型实现 `Sync`，可多线程共享；
/// - **超时保护**：每次请求带超时（默认 [`DEFAULT_TIMEOUT_MS`]，可用
///   [`NamedPipeClient::set_timeout_ms`] 调整）；超时后连接标记失效；
/// - **析构**：`Drop` 自动关闭句柄；该类型可跨线程移动（`Send`）。
pub struct NamedPipeClient {
    /// 管道句柄（Windows HANDLE）。
    handle: HANDLE,
    /// 写锁：把「WriteFile + ReadFile」整段串行化，保证请求响应一一对应。
    write_lock: Arc<Mutex<()>>,
    /// 连接是否已失效（超时触发）。失效后所有请求立即报错，需重新 `initialize`。
    broken: Arc<AtomicBool>,
    /// 单次请求的响应超时（毫秒）。
    timeout_ms: u64,
}

impl NamedPipeClient {
    /// 根据管道名连接 MT5 终端（使用默认超时 [`DEFAULT_TIMEOUT_MS`]）。
    ///
    /// # 参数
    ///
    /// - `pipe_name`：完整的管道路径，如 `\\.\pipe\MT5.Terminal.<HASH>`。
    ///   传 `None` 会返回 [`Mt5Error::ConnectionFailed`]（提示应先调用 `initialize`）。
    ///
    /// # 实现逻辑
    ///
    /// 1. 先调用 `WaitNamedPipeW` 等待管道可用（超时 500 毫秒）；
    /// 2. 再以 `GENERIC_READ | GENERIC_WRITE` 模式调用 `CreateFileW` 打开管道；
    /// 3. 打开失败（返回 `INVALID_HANDLE_VALUE`）时返回连接失败错误；
    /// 4. **把读取模式设为消息模式**（`PIPE_READMODE_MESSAGE`）——
    ///    MT5 是消息模式管道，不设置会导致 `WriteFile` 永久阻塞。
    pub fn new(pipe_name: Option<&str>) -> Result<Self> {
        Self::with_timeout(pipe_name, DEFAULT_TIMEOUT_MS)
    }

    /// 与 [`NamedPipeClient::new`] 相同，但可自定义响应超时（毫秒）。
    ///
    /// `timeout_ms` 传 0 表示「不启用超时」（退化为直接阻塞等待，
    /// 仅在明确知道终端不会假死时使用）。
    pub fn with_timeout(pipe_name: Option<&str>, timeout_ms: u64) -> Result<Self> {
        let name = match pipe_name {
            Some(n) => n.to_string(),
            None => {
                return Err(Mt5Error::ConnectionFailed(
                    "Pipe name must be provided. Use initialize(Some(\"pipe_name\"))".into(),
                ))
            }
        };

        // 将管道名转为以 NUL 结尾的 UTF-16LE 序列（Windows API 要求）
        let pipe_name_wide: Vec<u16> = name
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();

        unsafe {
            // 等待管道就绪（最多 500 毫秒），MT5 终端未启动时此处不会立刻失败
            WaitNamedPipeW(pipe_name_wide.as_ptr(), 500);
        }

        let handle = unsafe {
            // 以读写方式打开已存在的命名管道
            CreateFileW(
                pipe_name_wide.as_ptr(),
                0x80000000 | 0x40000000, // GENERIC_READ | GENERIC_WRITE
                0,                       // 不允许共享
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                std::ptr::null_mut(),
            )
        };

        if handle == INVALID_HANDLE_VALUE {
            return Err(Mt5Error::ConnectionFailed(format!(
                "Failed to connect to pipe: {}",
                name
            )));
        }

        // 关键：MT5 终端管道是「消息模式」管道。客户端必须显式设置
        // PIPE_READMODE_MESSAGE，否则 WriteFile 会永久阻塞（字节模式写消息管道）。
        // Python MetaTrader5 / go-mt5 均做此设置。
        unsafe {
            let mode: u32 = PIPE_READMODE_MESSAGE;
            SetNamedPipeHandleState(handle, &mode, std::ptr::null(), std::ptr::null());
        }

        Ok(Self {
            handle,
            write_lock: Arc::new(Mutex::new(())),
            broken: Arc::new(AtomicBool::new(false)),
            timeout_ms,
        })
    }

    /// 设置单次请求的响应超时（毫秒；0 = 不启用超时）。
    ///
    /// 可在同一个连接上动态调整，例如对「只读快照」用较短超时、
    /// 对「下单」用较长超时。
    pub fn set_timeout_ms(&mut self, timeout_ms: u64) {
        self.timeout_ms = timeout_ms;
    }

    /// 读取当前配置的响应超时（毫秒）。
    pub fn timeout_ms(&self) -> u64 {
        self.timeout_ms
    }

    /// 连接是否已失效（此前发生过超时）。
    ///
    /// 失效后 [`NamedPipeClient::send`] 会立即返回错误，
    /// 调用方应重新 [`crate::Mt5Client::initialize`]。
    pub fn is_broken(&self) -> bool {
        self.broken.load(Ordering::SeqCst)
    }

    /// 向管道发送一条命令并同步等待完整响应。
    ///
    /// # 参数
    ///
    /// - `cmd`：MT5 命令码（如 190=账户信息、170=品种信息，具体见 [`crate::Mt5Client`] 各方法）；
    /// - `data`：命令参数（原始字节，字符串需先用 UTF-16LE 编码）。
    ///
    /// # 返回
    ///
    /// 响应帧的「返回数据」部分（8 字节响应头之后的载荷字节）。
    ///
    /// # 错误处理
    ///
    /// - 终端拒绝命令（成功标志为 0）→ [`Mt5Error::CommandFailed`]（含错误码与描述）；
    /// - 等待响应超时 → [`Mt5Error::Timeout`]，且连接被标记失效（[`NamedPipeClient::is_broken`]）；
    /// - 连接已失效 → [`Mt5Error::ConnectionFailed`]（提示重新 initialize）；
    /// - 底层读写失败 → [`Mt5Error::IoError`]。
    ///
    /// # 并发与超时
    ///
    /// 「写 + 读」在独立线程内持有写锁执行，本线程按 [`NamedPipeClient::timeout_ms`]
    /// 等待结果；因此本方法可被多线程同时调用（请求会被串行化），且不会永久阻塞。
    pub fn send(&self, cmd: u32, data: &[u8]) -> Result<Vec<u8>> {
        // 连接已失效：不再发送，避免在残破管道上继续读写
        if self.broken.load(Ordering::SeqCst) {
            return Err(Mt5Error::ConnectionFailed(
                "连接已失效（此前发生过超时），请重新 initialize".into(),
            ));
        }

        // 超时未启用：直接在当前线程完成「写 + 读」（仍是串行的）
        if self.timeout_ms == 0 {
            let _guard = self.write_lock.lock().unwrap_or_else(|e| e.into_inner());
            return unsafe { sync_request(self.handle, cmd, data) };
        }

        // 启用超时：后台线程执行「写 + 读」，本线程限时等待
        let (tx, rx) = mpsc::channel::<Result<Vec<u8>>>();
        let handle: usize = self.handle as usize;
        let write_lock = Arc::clone(&self.write_lock);
        let payload = data.to_vec();
        std::thread::spawn(move || {
            let _guard = write_lock.lock().unwrap_or_else(|e| e.into_inner());
            let result = unsafe { sync_request(handle as HANDLE, cmd, &payload) };
            let _ = tx.send(result);
        });

        match rx.recv_timeout(Duration::from_millis(self.timeout_ms)) {
            Ok(result) => result,
            Err(_) => {
                // 超时：管道里可能残留本次响应，若继续复用会与下一次请求错配
                // （串包）。因此把连接标记为失效，调用方重新 initialize 即可恢复。
                self.broken.store(true, Ordering::SeqCst);
                Err(Mt5Error::Timeout(format!(
                    "{}ms 内未收到终端响应；连接已标记失效，请重新 initialize",
                    self.timeout_ms
                )))
            }
        }
    }
}

/// 在已打开的消息模式管道上执行一次「写请求帧 → 读响应帧」。
///
/// # 安全
///
/// **必须**在持有 `write_lock` 的线程内调用：同一管道句柄上并发
/// `ReadFile` / `WriteFile` 会永久阻塞，且会让请求与响应错配。
unsafe fn sync_request(handle: HANDLE, cmd: u32, data: &[u8]) -> Result<Vec<u8>> {
    // 1) 写请求帧：[总长度 u32][命令码 u32][参数]
    let total_len = 4 + data.len();
    let mut request = Vec::with_capacity(8 + data.len());
    request.extend_from_slice(&(total_len as u32).to_le_bytes());
    request.extend_from_slice(&cmd.to_le_bytes());
    request.extend_from_slice(data);

    let mut bytes_written = 0u32;
    let ok = unsafe {
        WriteFile(
            handle,
            request.as_ptr(),
            request.len() as u32,
            &mut bytes_written,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Err(Mt5Error::IoError(std::io::Error::last_os_error()));
    }

    // 2) 读载荷长度（4 字节）
    let mut len_buf = [0u8; 4];
    let mut bytes_read = 0u32;
    let ok = unsafe {
        ReadFile(
            handle,
            len_buf.as_mut_ptr(),
            len_buf.len() as u32,
            &mut bytes_read,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Err(Mt5Error::IoError(std::io::Error::last_os_error()));
    }

    let payload_len = u32::from_le_bytes(len_buf) as usize;
    if payload_len < 8 {
        // 响应头本身占 8 字节（命令码 + 成功标志），不足即为非法帧
        return Err(Mt5Error::InvalidResponse(format!(
            "Payload too small: {} bytes",
            payload_len
        )));
    }

    // 3) 循环读满整个载荷（管道读不保证一次读全）
    let mut payload = vec![0u8; payload_len];
    let mut total_read = 0usize;
    while total_read < payload_len {
        let mut n = 0u32;
        let ok = unsafe {
            ReadFile(
                handle,
                payload[total_read..].as_mut_ptr(),
                (payload_len - total_read) as u32,
                &mut n,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(Mt5Error::IoError(std::io::Error::last_os_error()));
        }
        total_read += n as usize;
    }

    // 4) 解析响应头：命令码 + 成功标志
    let success = u32::from_le_bytes([payload[4], payload[5], payload[6], payload[7]]) != 0;
    let body = if payload.len() > 8 {
        payload[8..].to_vec()
    } else {
        Vec::new()
    };

    if !success {
        // 终端返回失败：数据区格式 [错误码 i32][错误消息：4 字节字符数 + UTF-16LE]
        let mut code = -1i32;
        if body.len() >= 4 {
            code = i32::from_le_bytes([body[0], body[1], body[2], body[3]]);
        }
        let msg = decode_error_message(if body.len() > 4 { &body[4..] } else { &[] });
        return Err(Mt5Error::CommandFailed {
            cmd,
            error_code: code,
            error: msg,
        });
    }

    Ok(body)
}

/// 析构时自动关闭管道句柄，避免句柄泄漏。
impl Drop for NamedPipeClient {
    fn drop(&mut self) {
        if self.handle != INVALID_HANDLE_VALUE {
            unsafe {
                CloseHandle(self.handle);
            }
        }
    }
}

/// 管道客户端可以安全地跨线程移动（句柄本身由 Windows 管理生命周期）。
unsafe impl Send for NamedPipeClient {}

/// # 安全
///
/// `NamedPipeClient` 内部的**所有**管道 IO 都在 `write_lock` 内串行执行，
/// 因此多个线程同时调用（只读 `&self`）不会并发 `ReadFile`/`WriteFile`，
/// 请求与响应也不会错配；`broken` 为原子量，`timeout_ms` 仅在 `&mut self`
/// 下修改——故 `Sync` 成立。
unsafe impl Sync for NamedPipeClient {}

/// 根据 MT5 终端安装路径计算其命名管道的名称。
///
/// # 实现逻辑（与 Python `MetaTrader5` 库 / `go-mt5` 完全一致）
///
/// 1. 将终端路径转为小写，并加上 `\\?\` 前缀（Windows 长路径前缀）；
/// 2. 将该字符串按 UTF-16LE 编码为字节序列；
/// 3. 对字节序列计算 SHA-256 哈希；
/// 4. 管道名 = `\\.\pipe\MT5.Terminal.` + 哈希的大写十六进制字符串。
pub fn compute_pipe_name(terminal_path: &str) -> String {
    let input = format!(r"\\?\{}", terminal_path.to_lowercase());
    let input_utf16: Vec<u16> = input.encode_utf16().collect();

    // 将 UTF-16 字符序列转换为小端字节序列（低字节在前）
    let mut buf = Vec::with_capacity(input_utf16.len() * 2);
    for c in input_utf16 {
        buf.push(c as u8);
        buf.push((c >> 8) as u8);
    }

    let mut hasher = Sha256::new();
    hasher.update(&buf);
    let result = hasher.finalize();

    format!(r"\\.\pipe\MT5.Terminal.{}", hex::encode(result).to_uppercase())
}

/// 自动发现正在运行的 MT5 终端对应的管道名称（**单终端**便捷入口）。
///
/// # 实现逻辑
///
/// 1. 枚举系统中所有名为 `terminal64.exe` 的进程；
/// 2. 逐个取其可执行文件完整路径，计算对应的管道名；
/// 3. 依次测试各管道是否可连接，返回**第一个**可连接的管道名；
/// 4. 若没有任何管道可连接，直接 `panic!`（调用方应确认 MT5 已启动）。
///
/// > 多终端场景请用 [`discover_all_terminals`] / [`discover_all_mt5_pipes`]。
pub fn discover_mt5_pipe() -> String {
    let paths = find_terminal64_paths().unwrap_or_default();

    for path in &paths {
        let pipe_name = compute_pipe_name(path);
        if test_pipe_connection(&pipe_name) {
            return pipe_name;
        }
    }

    panic!("No responding MT5 pipe found");
}

/// 发现本机全部正在运行的 MT5 终端，返回 `(terminal64.exe 完整路径, 命名管道名)`。
///
/// # 用途
///
/// 多账户 / 多实例场景：每个终端一个管道，且能拿到**安装路径**，
/// 便于进一步定位终端目录（data 目录、`MQL5\Experts` 等）做部署或日志定位。
///
/// # 行为
///
/// - 只返回**当前可连接**的终端（枚举到的进程但管道打不开会被跳过）；
/// - 找不到终端进程或全部不可连接时返回**空列表**（不 panic）；
/// - 路径已去重（同一 `terminal64.exe` 路径只出现一次）。
///
/// # 示例
///
/// ```no_run
/// use mt5_rs::discover_all_terminals;
///
/// for (exe_path, pipe_name) in discover_all_terminals() {
///     println!("终端: {exe_path}\n  管道: {pipe_name}");
/// }
/// ```
pub fn discover_all_terminals() -> Vec<(String, String)> {
    let Ok(paths) = find_terminal64_paths() else {
        return Vec::new();
    };

    let mut terminals = Vec::new();
    for path in paths {
        let pipe_name = compute_pipe_name(&path);
        if test_pipe_connection(&pipe_name) {
            terminals.push((path, pipe_name));
        }
    }
    terminals
}

/// 发现本机全部正在运行的 MT5 终端的命名管道（**多实例**场景用）。
///
/// 等价于 [`discover_all_terminals`] 只取管道名；找不到时返回空列表。
///
/// # 示例
///
/// ```no_run
/// use mt5_rs::discover_all_mt5_pipes;
///
/// let pipes = discover_all_mt5_pipes();
/// println!("发现 {} 个在线 MT5 终端", pipes.len());
/// ```
pub fn discover_all_mt5_pipes() -> Vec<String> {
    discover_all_terminals()
        .into_iter()
        .map(|(_, pipe)| pipe)
        .collect()
}

/// 测试指定管道当前是否可连接（尝试以读写模式打开，成功即关闭并返回 `true`）。
fn test_pipe_connection(pipe_name: &str) -> bool {
    let pipe_name_wide: Vec<u16> = pipe_name
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        WaitNamedPipeW(pipe_name_wide.as_ptr(), 500);
    }

    let handle = unsafe {
        CreateFileW(
            pipe_name_wide.as_ptr(),
            0x80000000 | 0x40000000,
            0,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            std::ptr::null_mut(),
        )
    };

    if handle != INVALID_HANDLE_VALUE {
        unsafe {
            CloseHandle(handle);
        }
        true
    } else {
        false
    }
}

/// 枚举所有正在运行的 `terminal64.exe` 进程，返回其可执行文件完整路径列表（去重）。
///
/// # 实现逻辑
///
/// 1. 调用 `CreateToolhelp32Snapshot` 创建进程快照；
/// 2. 用 `Process32FirstW` / `Process32NextW` 遍历进程；
/// 3. 进程名（小写）为 `terminal64.exe` 时，用 `get_process_path` 取完整路径并入集合去重；
/// 4. 找不到任何终端进程时返回连接失败错误。
pub fn find_terminal64_paths() -> Result<Vec<String>> {
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Err(Mt5Error::ConnectionFailed(
            "Failed to create process snapshot".into(),
        ));
    }

    let mut paths = Vec::new();
    let mut seen = std::collections::HashSet::new();

    let mut pe = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..unsafe { std::mem::zeroed() }
    };

    let mut result = unsafe { Process32FirstW(snapshot, &mut pe) };
    while result != 0 {
        // 进程可执行文件名（含 NUL 结尾，需去除）
        let exe_name = String::from_utf16_lossy(&pe.szExeFile)
            .trim_end_matches('\0')
            .to_lowercase();

        if exe_name == "terminal64.exe" {
            if let Ok(path) = get_process_path(pe.th32ProcessID) {
                if seen.insert(path.clone()) {
                    paths.push(path);
                }
            }
        }

        result = unsafe { Process32NextW(snapshot, &mut pe) };
    }

    unsafe { CloseHandle(snapshot) };

    if paths.is_empty() {
        return Err(Mt5Error::ConnectionFailed(
            "No running terminal64.exe found".into(),
        ));
    }

    Ok(paths)
}

/// 根据进程 ID 获取其可执行文件的完整路径。
///
/// # 实现逻辑
///
/// 1. 以 `PROCESS_QUERY_LIMITED_INFORMATION` 权限 `OpenProcess` 打开进程句柄；
/// 2. 调用 `QueryFullProcessImageNameW` 查询完整路径（缓冲区 32768 个 UTF-16 字符）；
/// 3. 无论成功与否都关闭句柄，失败时返回连接失败错误。
fn get_process_path(pid: u32) -> Result<String> {
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle == std::ptr::null_mut() {
        return Err(Mt5Error::ConnectionFailed(format!(
            "Failed to open process {}",
            pid
        )));
    }

    let mut buf = [0u16; 32768];
    let mut size = buf.len() as u32;

    let result = unsafe { QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut size) };

    unsafe { CloseHandle(handle) };

    if result == 0 {
        return Err(Mt5Error::ConnectionFailed(format!(
            "Failed to get process image name for PID {}",
            pid
        )));
    }

    Ok(String::from_utf16_lossy(&buf[..size as usize]))
}

/// 解码终端错误消息（格式：`[4 字节字符数][UTF-16LE 字符序列]`，NUL 截断，容错）。
///
/// 数据不足或格式异常时返回空字符串（不报错），保证错误路径本身不失败。
fn decode_error_message(data: &[u8]) -> String {
    if data.len() < 4 {
        return String::new();
    }
    let char_count = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    let byte_count = char_count.saturating_mul(2).min(data.len().saturating_sub(4));
    let mut chars = Vec::with_capacity(byte_count / 2);
    let mut i = 4;
    let end = 4 + byte_count;
    while i + 1 < end {
        let c = u16::from_le_bytes([data[i], data[i + 1]]);
        if c == 0 {
            break;
        }
        chars.push(c);
        i += 2;
    }
    String::from_utf16_lossy(&chars)
}
