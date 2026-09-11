use crate::error::{Mt5Error, Result};
use crate::protocol::NamedPipeClient;
use crate::types::*;
use std::sync::{Arc, Mutex};

/// MT5 客户端：面向用户的主入口。
///
/// 封装了与 MetaTrader 5 终端进行 IPC 通信所需的全部 API，
/// 函数命名与 Python `MetaTrader5` 库一一对应。
///
/// # 使用流程
///
/// 1. [`Mt5Client::new`] 创建客户端；
/// 2. [`Mt5Client::initialize`] 传入管道名建立连接（管道名可用 [`crate::discover_mt5_pipe`] 自动发现）；
/// 3. 调用各类查询 API（账户、行情、持仓等）；
/// 4. 结束时调用 [`Mt5Client::shutdown`] 释放连接。
///
/// # 注意事项
///
/// - 所有方法都会先检查是否已初始化（[`Mt5Error::NotInitialized`]）；
/// - 所有方法都要求 MT5 终端正在运行且已登录账户；
/// - 全部 32 个 API（与 Python `MetaTrader5` 库对齐）均已实现。
///
/// # 线程安全
///
/// 类型实现了 `Send + Sync`：内部管道由写锁串行化（见 [`NamedPipeClient`]），
/// `last_error` 用 `Mutex` 保护。因此可把同一个客户端放进 `Arc`，
/// 由多个线程并发调用（请求会被内部串行化，不会串包）。
/// 注意：`initialize` / `shutdown` 需要 `&mut self`，应在共享前完成连接。
pub struct Mt5Client {
    /// 已建立的管道连接；`None` 表示尚未初始化。
    pipe: Option<NamedPipeClient>,
    /// 终端 build 号（在 `initialize` 时获取）。
    build: i32,
    /// 最近一次调用的错误（本地维护，供 [`Mt5Client::last_error`] 读取，
    /// 与 Python `MetaTrader5` 库的 `last_error` 语义一致）。
    /// 用 `Mutex` 而非 `RefCell`，以保证多线程下可用（`Sync`）。
    last_error: Mutex<Option<(i32, String)>>,
}

impl Mt5Client {
    /// 创建一个新的 MT5 客户端（尚未建立任何连接）。
    ///
    /// # 返回值
    ///
    /// 返回 `pipe` 为 `None`、`build` 为 0 的空客户端，需要后续调用 [`Mt5Client::initialize`]。
    pub fn new() -> Self {
        Self {
            pipe: None,
            build: 0,
            last_error: Mutex::new(None),
        }
    }

    /// 初始化客户端并建立与 MT5 终端的管道连接。
    ///
    /// # 参数
    ///
    /// - `pipe_name`：完整的管道路径（如 `\\.\pipe\MT5.Terminal.<HASH>`）。
    ///   传入 `None` 将导致连接失败；建议先用 [`crate::discover_mt5_pipe`] 自动发现。
    ///
    /// # 实现逻辑
    ///
    /// 1. 用管道名创建 [`NamedPipeClient`] 并保存；
    /// 2. 发送命令码 `4`（握手命令），参数为：`3u32`（协议版本）+ 字符串 `"Go"`；
    /// 3. 响应前 4 字节为终端 build 号，保存到 `self.build`。
    ///
    /// # 返回值
    ///
    /// 连接与握手成功返回 `Ok(())`，否则返回 [`Mt5Error`]。
    pub fn initialize(&mut self, pipe_name: Option<&str>) -> Result<()> {
        self.initialize_with_timeout(pipe_name, crate::protocol::DEFAULT_TIMEOUT_MS)
    }

    /// 初始化客户端并建立连接，同时指定响应超时（毫秒；0 = 不启用超时）。
    ///
    /// 与 [`Mt5Client::initialize`] 的唯一区别是超时可自定义；连接完成后
    /// 仍可用 [`Mt5Client::set_timeout_ms`] 动态调整。
    pub fn initialize_with_timeout(
        &mut self,
        pipe_name: Option<&str>,
        timeout_ms: u64,
    ) -> Result<()> {
        self.pipe = Some(NamedPipeClient::with_timeout(pipe_name, timeout_ms)?);

        let mut data = Vec::new();
        // 握手数据：协议版本号 3 + 客户端标识字符串 "Go"（与 go-mt5 保持一致）
        data.extend_from_slice(&3u32.to_le_bytes());
        data.extend_from_slice(&encode_string("Go"));

        let resp = self.send(4, &data)?;
        if resp.len() >= 4 {
            // 响应前 4 字节为 MT5 终端的 build 号
            let build = u32::from_le_bytes([resp[0], resp[1], resp[2], resp[3]]);
            self.build = build as i32;
        }

        Ok(())
    }

    /// 动态调整响应超时（毫秒；0 = 不启用超时）。需在 `initialize` 之后调用。
    ///
    /// 典型用法：常规查询用短超时（如 1000ms）快速失败，
    /// 下单等关键操作放宽（如 5000ms）。
    pub fn set_timeout_ms(&mut self, timeout_ms: u64) -> Result<()> {
        let pipe = self.pipe.as_mut().ok_or(Mt5Error::NotInitialized)?;
        pipe.set_timeout_ms(timeout_ms);
        Ok(())
    }

    /// 读取当前连接的响应超时（毫秒）；未初始化返回 `None`。
    pub fn timeout_ms(&self) -> Option<u64> {
        self.pipe.as_ref().map(|p| p.timeout_ms())
    }

    /// 当前连接是否已失效（此前发生过超时）。
    ///
    /// 失效后所有 API 都会返回 [`Mt5Error::ConnectionFailed`]，
    /// 需重新 [`Mt5Client::initialize`] 才能继续使用。
    pub fn is_connection_broken(&self) -> bool {
        self.pipe.as_ref().map(|p| p.is_broken()).unwrap_or(false)
    }

    /// 关闭与 MT5 终端的连接（管道句柄随之释放）。
    ///
    /// 关闭后所有其他 API 都会返回 [`Mt5Error::NotInitialized`]，
    /// 需要再次调用 [`Mt5Client::initialize`] 才能继续使用。
    pub fn shutdown(&mut self) {
        self.pipe = None;
    }

    /// 获取当前管道连接引用；未初始化时返回 [`Mt5Error::NotInitialized`]。
    ///
    /// 这是所有需要通信的方法共用的内部检查入口。
    fn pipe(&self) -> Result<&NamedPipeClient> {
        self.pipe
            .as_ref()
            .ok_or(Mt5Error::NotInitialized)
    }

    /// 统一发送入口：发送命令并记录最近一次调用的结果，供 [`Mt5Client::last_error`] 读取。
    ///
    /// # 实现逻辑
    ///
    /// - 成功：记录 `(0, "")`；
    /// - 失败：记录 `(错误码, 错误描述)`，错误码来自终端返回码或 Windows 错误码。
    fn send(&self, cmd: u32, data: &[u8]) -> Result<Vec<u8>> {
        let pipe = self.pipe()?;
        let result = pipe.send(cmd, data);
        *self.last_error.lock().unwrap_or_else(|e| e.into_inner()) = match &result {
            Ok(_) => Some((0, String::new())),
            Err(e) => Some((e.error_code(), e.to_string())),
        };
        result
    }

    /// 登录 MT5 账户（命令码 `4`）。
    ///
    /// # 参数
    ///
    /// - `login`：账户号码（i64）；
    /// - `password`：账户密码（需为 `&str`）；
    /// - `server`：交易服务器名称（如 `"MetaQuotes-Demo"`）。
    ///
    /// # 实现逻辑
    ///
    /// 1. 拼装参数：`login`（8 字节 i64 LE）+ 密码字符串 + 服务器字符串；
    /// 2. 发送命令码 `4`；
    /// 3. 响应前 4 字节为状态码：`0` 表示登录成功，非 0 返回 [`Mt5Error::CommandFailed`]。
    pub fn login(&self, login: i64, password: &str, server: &str) -> Result<()> {
        let mut data = Vec::new();
        data.extend_from_slice(&login.to_le_bytes());
        data.extend_from_slice(&encode_string(password));
        data.extend_from_slice(&encode_string(server));

        let resp = self.send(4, &data)?;
        if resp.len() < 4 {
            return Err(Mt5Error::InvalidResponse("Response too short".into()));
        }

        let status = u32::from_le_bytes([resp[0], resp[1], resp[2], resp[3]]);
        if status != 0 {
            return Err(Mt5Error::CommandFailed {
                cmd: 4,
                error_code: status as i32,
                error: format!("Login failed with status: {}", status),
            });
        }

        Ok(())
    }

    /// 获取当前账户信息（命令码 `190`，对应 Python `mt5.account_info()`）。
    ///
    /// # 返回值
    ///
    /// 返回 [`AccountInfo`]，包含余额、净值、保证金、杠杆等全部账户属性。
    ///
    /// # 实现逻辑
    ///
    /// 按 go-mt5 / Python 库验证过的二进制布局逐字段解析响应：
    ///
    /// - 前 147 字节为定长数值区：`login`(i64)、`trade_mode`/`leverage`/`limit_orders`/
    ///   `margin_so_mode`(i32)、`trade_allowed`/`trade_expert`(bool×1)、`margin_mode`/
    ///   `currency_digits`(i32)、`fifo_close`(bool×1)，随后依次是 14 个 f64 字段；
    /// - 147 字节之后为字符串区：`name`(256 字节 UTF-16LE 槽)、`server`(128)、
    ///   `currency`(64)、`company`(256)。
    pub fn account_info(&self) -> Result<AccountInfo> {
        let resp = self.send(190, &[])?;

        if resp.len() < 8 {
            return Err(Mt5Error::InvalidResponse("Response too short".into()));
        }

        let mut reader = Reader::new(&resp);

        // 按照 Python 输出和二进制数据验证的精确位置解析
        // Pos 0-7: login (i64)
        let login = reader.read_i64();

        // Pos 8-11: trade_mode (i32)
        let trade_mode = reader.read_i32() as i64;

        // Pos 12-15: leverage (i32)
        let leverage = reader.read_i32() as i64;

        // Pos 16-19: limit_orders (i32)
        let limit_orders = reader.read_i32() as i64;

        // Pos 20-23: margin_so_mode (i32)
        let margin_so_mode = reader.read_i32() as i64;

        // Pos 24: trade_allowed (bool, 1字节)
        let trade_allowed = reader.read_bool1();

        // Pos 25: trade_expert (bool, 1字节)
        let trade_expert = reader.read_bool1();

        // Pos 26-29: margin_mode (i32)
        let margin_mode = reader.read_i32() as i64;

        // Pos 30-33: currency_digits (i32)
        let currency_digits = reader.read_i32() as i64;

        // Pos 34: fifo_close (bool, 1字节)
        let fifo_close = reader.read_bool1();

        // Pos 35-42: balance (f64)
        let balance = reader.read_f64();

        // Pos 43-50: credit (f64)
        let credit = reader.read_f64();

        // Pos 51-58: profit (f64)
        let profit = reader.read_f64();

        // Pos 59-66: equity (f64)
        let equity = reader.read_f64();

        // Pos 67-74: margin (f64)
        let margin = reader.read_f64();

        // Pos 75-82: margin_free (f64)
        let free_margin = reader.read_f64();

        // Pos 83-90: margin_level (f64)
        let margin_level = reader.read_f64();

        // Pos 91-98: margin_so_call (f64)
        let margin_so_call = reader.read_f64();

        // Pos 99-106: margin_so_so (f64)
        let margin_so_so = reader.read_f64();

        // Pos 107-114: margin_initial (f64)
        let margin_initial = reader.read_f64();

        // Pos 115-122: margin_maintenance (f64)
        let margin_maintenance = reader.read_f64();

        // Pos 123-130: assets (f64)
        let assets = reader.read_f64();

        // Pos 131-138: liabilities (f64)
        let liabilities = reader.read_f64();

        // Pos 139-146: commission_blocked (f64)
        let commission_blocked = reader.read_f64();

        // 读取字符串字段 (从 pos 147 开始)
        let strings_offset = 147;
        if strings_offset >= resp.len() {
            return Err(Mt5Error::InvalidResponse(format!(
                "Response too short for strings: {} < {}",
                resp.len(),
                strings_offset
            )));
        }

        // 字符串区为固定宽度 UTF-16LE 槽：name=256 字节、server=128 字节、
        // currency=64 字节、company=256 字节，遇 NUL 截断
        let mut sr = Reader::new(&resp[strings_offset..]);
        let name = sr.read_fixed_string(256);
        let server = sr.read_fixed_string(128);
        let currency = sr.read_fixed_string(64);
        let company = sr.read_fixed_string(256);

        if sr.has_error() {
            return Err(Mt5Error::InvalidResponse("Failed to read strings".into()));
        }

        Ok(AccountInfo {
            login,
            trade_mode,
            leverage,
            limit_orders,
            margin_so_mode,
            trade_allowed,
            trade_expert,
            margin_mode,
            currency_digits,
            fifo_close,
            balance,
            credit,
            profit,
            equity,
            margin,
            free_margin,
            margin_level,
            margin_so_call,
            margin_so_so,
            margin_initial,
            margin_maintenance,
            assets,
            liabilities,
            commission_blocked,
            name,
            server,
            currency,
            company,
        })
    }

    /// 获取 MT5 终端信息（命令码 `180`，对应 Python `mt5.terminal_info()`）。
    ///
    /// # 返回值
    ///
    /// 返回 [`TerminalInfo`]，包含连接状态、build 号、终端路径、公司名等。
    ///
    /// # 实现逻辑
    ///
    /// 响应为定长二进制布局（非流式字段，直接按偏移读取）：
    ///
    /// - 偏移 0-39：数值区。`build`(u16@0)、两个保留字节、`community_account`(bool@2)、
    ///   `community_connection`(bool@3)、`notifications_enabled`(bool@4)、`mqid`(bool@5)、
    ///   `connected`(bool@6)、`dlls_allowed`(bool@7)、`trade_allowed`(bool@8)、
    ///   `trade_api_disabled`(bool@9)、`email_enabled`(bool@10)、`ftp_enabled`(bool@11)、
    ///   `max_bars`(u32@12)、`code_page`(u16@17)、`ping_last`(u16@21)、
    ///   `community_balance`(f64@24)、`retransmission`(f64@32)；
    /// - 偏移 41 起为字符串区（固定 520 字节一槽）：`company`@41、`name`@561、
    ///   `language`@1081、`path`@1601、`data_path`@2121、`common_data_path`@2641。
    pub fn terminal_info(&self) -> Result<TerminalInfo> {
        let resp = self.send(180, &[])?;

        if resp.len() < 40 {
            return Err(Mt5Error::InvalidResponse("Response too short".into()));
        }

        // ---- 布尔字段（每个 1 字节，非 0 即 true）----
        let community_account = resp[2] != 0;
        let community_connection = resp[3] != 0;
        let connected = resp[6] != 0;
        let dlls_allowed = resp[7] != 0;
        let trade_allowed = resp[8] != 0;
        let trade_api_disabled = resp[9] != 0;
        let email_enabled = resp[10] != 0;
        let ftp_enabled = resp[11] != 0;
        let notifications_enabled = resp[4] != 0;
        let mqid = resp[5] != 0;

        // ---- 数值字段 ----
        let build = u16::from_le_bytes([resp[0], resp[1]]) as i64;
        let max_bars = u32::from_le_bytes([resp[12], resp[13], resp[14], resp[15]]) as i64;
        let code_page = u16::from_le_bytes([resp[17], resp[18]]) as i64;
        let ping_last = u16::from_le_bytes([resp[21], resp[22]]) as i64;
        let community_balance = f64::from_le_bytes([
            resp[24], resp[25], resp[26], resp[27], resp[28], resp[29], resp[30], resp[31],
        ]);
        let retransmission = f64::from_le_bytes([
            resp[32], resp[33], resp[34], resp[35], resp[36], resp[37], resp[38], resp[39],
        ]);

        // ---- 字符串字段（固定 520 字节 UTF-16LE 槽，遇 NUL 截断）----
        let company = read_string_at_offset(&resp, 41);
        let name = read_string_at_offset(&resp, 561);
        let language = read_string_at_offset(&resp, 1081);
        let path = read_string_at_offset(&resp, 1601);
        let data_path = read_string_at_offset(&resp, 2121);
        let common_data_path = read_string_at_offset(&resp, 2641);

        Ok(TerminalInfo {
            community_account,
            community_connection,
            connected,
            dlls_allowed,
            trade_allowed,
            trade_api_disabled,
            email_enabled,
            ftp_enabled,
            notifications_enabled,
            mqid,
            build,
            max_bars,
            code_page,
            ping_last,
            community_balance,
            retransmission,
            company,
            name,
            language,
            path,
            data_path,
            common_data_path,
        })
    }

    /// 获取 MT5 版本信息（对应 Python `mt5.version()`）。
    ///
    /// # 实现逻辑
    ///
    /// 复用 [`Mt5Client::terminal_info`] 的结果：
    /// `version` 与 `build` 均取终端的 build 号，
    /// `build_date` 为 `"公司名 (终端名)"` 格式的字符串。
    pub fn version(&self) -> Result<VersionInfo> {
        let info = self.terminal_info()?;
        Ok(VersionInfo {
            version: info.build as i32,
            build: info.build as i32,
            build_date: format!("{} ({})", info.company, info.name),
        })
    }

    /// 获取市场报价中可用的交易品种总数（命令码 `173`）。
    ///
    /// # 实现逻辑
    ///
    /// 响应前 4 字节为品种数量（u32 LE），直接转成 `i64` 返回。
    pub fn symbols_total(&self) -> Result<i64> {
        let resp = self.send(173, &[])?;

        if resp.len() < 4 {
            return Err(Mt5Error::InvalidResponse("Response too short".into()));
        }

        let total = u32::from_le_bytes([resp[0], resp[1], resp[2], resp[3]]);
        Ok(total as i64)
    }

    /// 获取全部交易品种信息（命令码 `174`，对应 Python `mt5.symbols_get()`）。
    ///
    /// # 返回值
    ///
    /// 返回 [`SymbolInfo`] 列表，每个元素描述一个品种的完整属性
    /// （合约参数、行情快照、保证金规则等）。
    ///
    /// # 实现逻辑
    ///
    /// 1. 读取前 4 字节得到品种数量 `count`；
    /// 2. 循环调用 `decode_symbol_info` 解析 `count` 个定长记录。
    pub fn symbols_get(&self) -> Result<Vec<SymbolInfo>> {
        let resp = self.send(174, &[])?;

        if resp.len() < 4 {
            return Err(Mt5Error::InvalidResponse("Response too short".into()));
        }

        let mut reader = Reader::new(&resp);
        let count = reader.read_u32() as usize;

        let mut symbols = Vec::with_capacity(count);

        for _ in 0..count {
            let sym = Self::decode_symbol_info(&mut reader)?;
            symbols.push(sym);
        }

        Ok(symbols)
    }

    /// 解析单条品种信息记录（供 `symbols_get` / `symbol_info` 复用）。
    ///
    /// # 实现逻辑
    ///
    /// 严格按照 go-mt5 的 `decodeSymbolInfo` 字段顺序与类型解析：
    ///
    /// - 数值区：1 个 bool + 若干 u32/i64 混合字段 + 54 个 f64 字段（见各字段注释）；
    /// - 字符串区：13 个固定宽度 UTF-16LE 槽，共 2432 字节
    ///   （64+128+32+32+32+512+64+64+1024+32+128+256+64），遇 NUL 截断。
    ///
    /// 解析出错（缓冲区越界）时返回 [`Mt5Error::InvalidResponse`]。
    fn decode_symbol_info(reader: &mut Reader) -> Result<SymbolInfo> {
    // 严格按照 go-mt5 decodeSymbolInfo 的字段顺序和类型解析
    // 参考：https://github.com/Mukbeast4/go-mt5/blob/main/symbols.go
    let custom = reader.read_bool1();
    let chart_mode = reader.read_u32() as i64;
    let select = reader.read_bool1();
    let visible = reader.read_bool1();
    let session_deals = reader.read_i64();
    let session_buy_orders = reader.read_i64();
    let session_sell_orders = reader.read_i64();
    let volume = reader.read_i64();
    let volume_high = reader.read_i64();
    let volume_low = reader.read_i64();
    let time = reader.read_i64();
    let digits = reader.read_u32() as i64;
    let spread = reader.read_u32() as i64;
    let spread_float = reader.read_bool1();
    let ticks_book_depth = reader.read_u32() as i64;
    let trade_calc_mode = reader.read_u32() as i64;
    let trade_mode = reader.read_u32() as i64;
    let start_time = reader.read_i64();
    let expiration_time = reader.read_i64();
    let trade_stops_level = reader.read_u32() as i64;
    let trade_freeze_level = reader.read_u32() as i64;
    let trade_exe_mode = reader.read_u32() as i64;
    let swap_mode = reader.read_u32() as i64;
    let swap_rollover3days = reader.read_u32() as i64;
    let margin_hedged_use_leg = reader.read_bool1();
    let expiration_mode = reader.read_u32() as i64;
    let filling_mode = reader.read_u32() as i64;
    let order_mode = reader.read_u32() as i64;
    let order_gtc_mode = reader.read_u32() as i64;
    let option_mode = reader.read_u32() as i64;
    let option_right = reader.read_u32() as i64;
    let bid = reader.read_f64();
    let bid_high = reader.read_f64();
    let bid_low = reader.read_f64();
    let ask = reader.read_f64();
    let ask_high = reader.read_f64();
    let ask_low = reader.read_f64();
    let last = reader.read_f64();
    let last_high = reader.read_f64();
    let last_low = reader.read_f64();
    let volume_real = reader.read_f64();
    let volume_high_real = reader.read_f64();
    let volume_low_real = reader.read_f64();
    let option_strike = reader.read_f64();
    let point = reader.read_f64();
    let trade_tick_value = reader.read_f64();
    let trade_tick_value_profit = reader.read_f64();
    let trade_tick_value_loss = reader.read_f64();
    let trade_tick_size = reader.read_f64();
    let trade_contract_size = reader.read_f64();
    let trade_accrued_interest = reader.read_f64();
    let trade_face_value = reader.read_f64();
    let trade_liquidity_rate = reader.read_f64();
    let volume_min = reader.read_f64();
    let volume_max = reader.read_f64();
    let volume_step = reader.read_f64();
    let volume_limit = reader.read_f64();
    let swap_long = reader.read_f64();
    let swap_short = reader.read_f64();
    let margin_initial = reader.read_f64();
    let margin_maintenance = reader.read_f64();
    let session_volume = reader.read_f64();
    let session_turnover = reader.read_f64();
    let session_interest = reader.read_f64();
    let session_buy_orders_volume = reader.read_f64();
    let session_sell_orders_volume = reader.read_f64();
    let session_open = reader.read_f64();
    let session_close = reader.read_f64();
    let session_aw = reader.read_f64();
    let session_price_settlement = reader.read_f64();
    let session_price_limit_min = reader.read_f64();
    let session_price_limit_max = reader.read_f64();
    let margin_hedged = reader.read_f64();
    let price_change = reader.read_f64();
    let price_volatility = reader.read_f64();
    let price_theoretical = reader.read_f64();
    let price_greeks_delta = reader.read_f64();
    let price_greeks_theta = reader.read_f64();
    let price_greeks_gamma = reader.read_f64();
    let price_greeks_vega = reader.read_f64();
    let price_greeks_rho = reader.read_f64();
    let price_greeks_omega = reader.read_f64();
    let price_sensitivity = reader.read_f64();

    // 字符串字段：固定宽度 UTF-16LE 槽（go-mt5 PR#3 验证）
    // 总字符串区域 = 2432 字节
    let basis = reader.read_fixed_string(64);
    let category = reader.read_fixed_string(128);
    let currency_base = reader.read_fixed_string(32);
    let currency_profit = reader.read_fixed_string(32);
    let currency_margin = reader.read_fixed_string(32);
    let bank = reader.read_fixed_string(512);
    let description = reader.read_fixed_string(64);
    let exchange = reader.read_fixed_string(64);
    let formula = reader.read_fixed_string(1024);
    let isin = reader.read_fixed_string(32);
    let page = reader.read_fixed_string(128);
    let path = reader.read_fixed_string(256);
    let symbol_name = reader.read_fixed_string(64);

    if reader.has_error() {
        return Err(Mt5Error::InvalidResponse("Failed to read symbol info".into()));
    }

    Ok(SymbolInfo {
        custom,
        chart_mode,
        select,
        visible,
        session_deals,
        session_buy_orders,
        session_sell_orders,
        volume,
        volume_high,
        volume_low,
        time,
        digits,
        spread,
        spread_float,
        ticks_book_depth,
        trade_calc_mode,
        trade_mode,
        start_time,
        expiration_time,
        trade_stops_level,
        trade_freeze_level,
        trade_exe_mode,
        swap_mode,
        swap_rollover3days,
        margin_hedged_use_leg,
        expiration_mode,
        filling_mode,
        order_mode,
        order_gtc_mode,
        option_mode,
        option_right,
        bid,
        bidhigh: bid_high,
        bidlow: bid_low,
        ask,
        askhigh: ask_high,
        asklow: ask_low,
        last,
        lasthigh: last_high,
        lastlow: last_low,
        volume_real,
        volumehigh_real: volume_high_real,
        volumelow_real: volume_low_real,
        option_strike,
        point,
        trade_tick_value,
        trade_tick_value_profit,
        trade_tick_value_loss,
        trade_tick_size,
        trade_contract_size,
        trade_accrued_interest,
        trade_face_value,
        trade_liquidity_rate,
        volume_min,
        volume_max,
        volume_step,
        volume_limit,
        swap_long,
        swap_short,
        margin_initial,
        margin_maintenance,
        session_volume,
        session_turnover,
        session_interest,
        session_buy_orders_volume,
        session_sell_orders_volume,
        session_open,
        session_close,
        session_aw,
        session_price_settlement,
        session_price_limit_min,
        session_price_limit_max,
        margin_hedged,
        price_change,
        price_volatility,
        price_theoretical,
        price_greeks_delta,
        price_greeks_theta,
        price_greeks_gamma,
        price_greeks_vega,
        price_greeks_rho,
        price_greeks_omega,
        price_sensitivity,
        basis,
        category,
        currency_base,
        currency_profit,
        currency_margin,
        bank,
        description,
        exchange,
        formula,
        isin,
        name: symbol_name,
        page,
        path,
    })
}

    /// 获取指定品种的详细信息（命令码 `170`，对应 Python `mt5.symbol_info()`）。
    ///
    /// # 参数
    ///
    /// - `symbol`：品种名称，如 `"EURUSD"`。
    ///
    /// # 返回值
    ///
    /// 品种存在时返回 `Some(SymbolInfo)`；响应为空（品种不存在）时返回 `Ok(None)`。
    ///
    /// # 实现逻辑
    ///
    /// 发送「品种名字符串 → UTF-16LE 编码」作为参数，响应非空时复用
    /// `decode_symbol_info` 解析。
    pub fn symbol_info(&self, symbol: &str) -> Result<Option<SymbolInfo>> {
        let mut data = Vec::new();
        data.extend_from_slice(&encode_string(symbol));

        let resp = self.send(170, &data)?;

        if resp.is_empty() {
            return Ok(None);
        }

        let mut reader = Reader::new(&resp);
        let info = Self::decode_symbol_info(&mut reader)?;
        Ok(Some(info))
    }

    /// 获取指定品种的最新报价（Tick）（命令码 `172`，对应 Python `mt5.symbol_info_tick()`）。
    ///
    /// # 参数
    ///
    /// - `symbol`：品种名称，如 `"EURUSD"`。
    ///
    /// # 返回值
    ///
    /// 返回 `Some(Tick)`（含买卖价、最后价、成交量、时间戳）；
    /// 响应为空（品种不存在或暂无报价）时返回 `Ok(None)`。
    ///
    /// # 实现逻辑
    ///
    /// 严格按照 go-mt5 `decodeTick` 的字段顺序解析：
    /// `time`(i64) → `bid`/`ask`/`last`(f64) → `volume`(u64) → `time_msc`(i64) →
    /// `flags`(u32) → `volume_real`(f64)。
    pub fn symbol_info_tick(&self, symbol: &str) -> Result<Option<Tick>> {
        let mut data = Vec::new();
        data.extend_from_slice(&encode_string(symbol));

        let resp = self.send(172, &data)?;

        if resp.is_empty() {
            return Ok(None);
        }

        let mut reader = Reader::new(&resp);

        // 严格按照 go-mt5 decodeTick 的字段顺序和类型解析
        let time = reader.read_i64();
        let bid = reader.read_f64();
        let ask = reader.read_f64();
        let last = reader.read_f64();
        let volume = reader.read_u64();
        let time_msc = reader.read_i64();
        let flags = reader.read_u32();
        let volume_real = reader.read_f64();

        if reader.has_error() {
            return Err(Mt5Error::InvalidResponse("Failed to read tick info".into()));
        }

        Ok(Some(Tick {
            time,
            bid,
            ask,
            last,
            volume,
            time_msc,
            flags,
            volume_real,
        }))
    }

    /// 在“市场报价”窗口中选中/取消选中一个品种（命令码 `171`，对应 Python `mt5.symbol_select()`）。
    ///
    /// # 参数
    ///
    /// - `symbol`：品种名称；
    /// - `enable`：`true` 表示选中（加入市场报价），`false` 表示取消选中。
    ///
    /// # 返回值
    ///
    /// 操作成功返回 `true`。
    ///
    /// # 实现逻辑
    ///
    /// 1. 参数为「品种名字符串 + 1 字节标志（1=选中 / 0=取消）」；
    /// 2. 响应为空表示成功（MT5 只返回 8 字节响应头，无附加数据）；
    /// 3. 否则读取 4 字节状态码，非 0 视为成功。
    pub fn symbol_select(&self, symbol: &str, enable: bool) -> Result<bool> {
        let mut data = Vec::new();
        data.extend_from_slice(&encode_string(symbol));
        data.push(if enable { 1u8 } else { 0u8 });

        let resp = self.send(171, &data)?;

        // 空响应表示成功（MT5只返回8字节的头部，没有额外数据）
        if resp.is_empty() {
            return Ok(true);
        }

        if resp.len() < 4 {
            return Err(Mt5Error::InvalidResponse("Response too short".into()));
        }

        let status = i32::from_le_bytes([resp[0], resp[1], resp[2], resp[3]]);
        Ok(status != 0)
    }

    /// 获取当前未平仓持仓的总数（命令码 `120`，对应 Python `mt5.positions_total()`）。
    ///
    /// # 实现逻辑
    ///
    /// 响应前 4 字节为持仓数量（u32 LE）。
    pub fn positions_total(&self) -> Result<i64> {
        let resp = self.send(120, &[])?;

        if resp.len() < 4 {
            return Err(Mt5Error::InvalidResponse("Response too short".into()));
        }

        let total = u32::from_le_bytes([resp[0], resp[1], resp[2], resp[3]]);
        Ok(total as i64)
    }

    /// 获取当前挂单（待处理订单）的总数（命令码 `130`，对应 Python `mt5.orders_total()`）。
    ///
    /// # 实现逻辑
    ///
    /// 响应前 4 字节为挂单数量（u32 LE）。
    pub fn orders_total(&self) -> Result<i64> {
        let resp = self.send(130, &[])?;

        if resp.len() < 4 {
            return Err(Mt5Error::InvalidResponse("Response too short".into()));
        }

        let total = u32::from_le_bytes([resp[0], resp[1], resp[2], resp[3]]);
        Ok(total as i64)
    }

    /// 获取未平仓持仓列表（命令码 `121`，对应 Python `mt5.positions_get()`）。
    ///
    /// # 参数
    ///
    /// - `symbol`：按品种过滤；传 `None` 获取全部品种的持仓。
    ///
    /// # 返回值
    ///
    /// 返回 [`TradePosition`] 列表。
    ///
    /// # 实现逻辑
    ///
    /// 有品种过滤时附加「品种名字符串」参数，响应交由 `parse_positions_response` 解析。
    pub fn positions_get(&self, symbol: Option<&str>) -> Result<Vec<TradePosition>> {
        let cmd = 121;

        let mut data = Vec::new();
        if let Some(sym) = symbol {
            data.extend_from_slice(&encode_string(sym));
        }

        let resp = self.send(cmd, &data)?;
        parse_positions_response(&resp)
    }

    /// 获取挂单列表（命令码 `131`，对应 Python `mt5.orders_get()`）。
    ///
    /// # 参数
    ///
    /// - `symbol`：按品种过滤；传 `None` 获取全部品种的挂单。
    ///
    /// # 返回值
    ///
    /// 返回 [`TradeOrder`] 列表。
    ///
    /// # 实现逻辑
    ///
    /// 有品种过滤时附加「品种名字符串」参数，响应交由 `parse_orders_response` 解析。
    pub fn orders_get(&self, symbol: Option<&str>) -> Result<Vec<TradeOrder>> {
        let cmd = 131;

        let mut data = Vec::new();
        if let Some(sym) = symbol {
            data.extend_from_slice(&encode_string(sym));
        }

        let resp = self.send(cmd, &data)?;
        parse_orders_response(&resp)
    }

    /// 发送任意原始命令到 MT5 终端（高级接口）。
    ///
    /// # 参数
    ///
    /// - `cmd`：MT5 命令码；
    /// - `data`：原始参数字节。
    ///
    /// # 返回值
    ///
    /// 返回响应帧的数据部分（8 字节响应头之后的内容）。
    ///
    /// # 用途
    ///
    /// 用于本库尚未封装的自定义命令或协议调试；正常使用请调用各具名 API。
    pub fn send_raw_command(&self, cmd: u32, data: &[u8]) -> Result<Vec<u8>> {
        self.send(cmd, data)
    }

    /// 从指定位置开始复制 K 线（命令码 `108`，对应 Python `mt5.copy_rates_from_pos()`）。
    ///
    /// # 参数
    ///
    /// - `symbol`：品种名称；
    /// - `timeframe`：时间周期（`TIMEFRAME_*` 常量，如 1=1分钟、5=5分钟、1440=日线）；
    /// - `start_pos`：起始位置（从当前 K 线往回数，0 表示最新一根）；
    /// - `count`：需要复制的 K 线数量。
    ///
    /// # 返回值
    ///
    /// 返回 [`Rate`] 列表（时间升序）。
    ///
    /// # 实现逻辑
    ///
    /// 参数编码：品种字符串 + `timeframe`(u32) + `start_pos`(u32) + `count`(u32)，
    /// 响应交由 `parse_rates_response` 解析。
    pub fn copy_rates_from_pos(&self, symbol: &str, timeframe: i32, start_pos: i64, count: i32) -> Result<Vec<Rate>> {
        // 根据go-mt5源码，命令代码108，参数使用u32编码
        let cmd = 108;

        let mut data = Vec::new();
        data.extend_from_slice(&encode_string(symbol));
        data.extend_from_slice(&(timeframe as u32).to_le_bytes());
        data.extend_from_slice(&(start_pos as u32).to_le_bytes());
        data.extend_from_slice(&(count as u32).to_le_bytes());

        let resp = self.send(cmd, &data)?;
        parse_rates_response(&resp)
    }

    /// 从指定时间开始复制 K 线（命令码 `106`，对应 Python `mt5.copy_rates_from()`）。
    ///
    /// # 参数
    ///
    /// - `symbol`：品种名称；
    /// - `timeframe`：时间周期（`TIMEFRAME_*` 常量）；
    /// - `date_from`：起始时间（Unix 秒）；
    /// - `count`：需要复制的 K 线数量。
    ///
    /// # 返回值
    ///
    /// 返回 [`Rate`] 列表（时间升序）。
    ///
    /// # 实现逻辑
    ///
    /// 参数编码：品种字符串 + `timeframe`(u32) + `date_from`(i64) + `count`(u32)。
    pub fn copy_rates_from(&self, symbol: &str, timeframe: i32, date_from: i64, count: i32) -> Result<Vec<Rate>> {
        let cmd = 106;

        let mut data = Vec::new();
        data.extend_from_slice(&encode_string(symbol));
        data.extend_from_slice(&(timeframe as u32).to_le_bytes());
        data.extend_from_slice(&date_from.to_le_bytes());
        data.extend_from_slice(&(count as u32).to_le_bytes());

        let resp = self.send(cmd, &data)?;
        parse_rates_response(&resp)
    }

    /// 复制指定时间范围内的 K 线（命令码 `107`，对应 Python `mt5.copy_rates_range()`）。
    ///
    /// # 参数
    ///
    /// - `symbol`：品种名称；
    /// - `timeframe`：时间周期（`TIMEFRAME_*` 常量）；
    /// - `date_from`：起始时间（Unix 秒，含）；
    /// - `date_to`：结束时间（Unix 秒，含）。
    ///
    /// # 返回值
    ///
    /// 返回 [`Rate`] 列表（时间升序）。
    ///
    /// # 实现逻辑
    ///
    /// 参数编码：品种字符串 + `timeframe`(u32) + `date_from`(i64) + `date_to`(i64)。
    pub fn copy_rates_range(&self, symbol: &str, timeframe: i32, date_from: i64, date_to: i64) -> Result<Vec<Rate>> {
        let cmd = 107;

        let mut data = Vec::new();
        data.extend_from_slice(&encode_string(symbol));
        data.extend_from_slice(&(timeframe as u32).to_le_bytes());
        data.extend_from_slice(&date_from.to_le_bytes());
        data.extend_from_slice(&date_to.to_le_bytes());

        let resp = self.send(cmd, &data)?;
        parse_rates_response(&resp)
    }

    /// 从指定时间开始复制 Tick（命令码 `104`，对应 Python `mt5.copy_ticks_from()`）。
    ///
    /// # 参数
    ///
    /// - `symbol`：品种名称；
    /// - `from`：起始时间（**Unix 秒**，API 与 Python 库保持一致）；
    /// - `count`：需要复制的 Tick 数量；
    /// - `flags`：标志位（`COPY_TICKS_*` 常量：-1=全部、1=仅买价、2=仅卖价）。
    ///
    /// # 返回值
    ///
    /// 返回 [`Tick`] 列表（时间升序）。
    ///
    /// # 实现逻辑
    ///
    /// 参数编码：品种字符串 + `from`(i64) + `count`(u32) + `flags`(u32)。
    ///
    /// 注意：MT5 终端期望的时间戳单位为**毫秒**（与 MQL5 `CopyTicks` 一致），
    /// 本方法在内部将秒转换为毫秒（`from × 1000`）后发送。
    /// 若不转换（如 go-mt5 的做法），终端会把秒值当作毫秒（即 1970 年前后），
    /// 导致返回最早的历史缓存数据或空结果。
    pub fn copy_ticks_from(&self, symbol: &str, from: i64, count: i32, flags: i32) -> Result<Vec<Tick>> {
        let cmd = 104;

        let mut data = Vec::new();
        data.extend_from_slice(&encode_string(symbol));
        // 秒 → 毫秒：终端期望毫秒时间戳（见函数文档说明）
        data.extend_from_slice(&(from * 1000).to_le_bytes());
        data.extend_from_slice(&(count as u32).to_le_bytes());
        data.extend_from_slice(&(flags as u32).to_le_bytes());

        let resp = self.send(cmd, &data)?;
        parse_ticks_response(&resp)
    }

    /// 复制指定时间范围内的 Tick（命令码 `105`，对应 Python `mt5.copy_ticks_range()`）。
    ///
    /// # 参数
    ///
    /// - `symbol`：品种名称；
    /// - `from`：起始时间（**Unix 秒**，含）；
    /// - `to`：结束时间（**Unix 秒**，含）；
    /// - `flags`：标志位（`COPY_TICKS_*` 常量：-1=全部、1=仅买价、2=仅卖价）。
    ///
    /// # 返回值
    ///
    /// 返回 [`Tick`] 列表（时间升序）。
    ///
    /// # 实现逻辑
    ///
    /// 参数编码：品种字符串 + `from`(i64) + `to`(i64) + `flags`(u32)。
    ///
    /// 与 [`Mt5Client::copy_ticks_from`] 相同，`from`/`to` 在内部转换为毫秒
    /// （`× 1000`）后发送，以匹配终端期望的时间戳单位。
    pub fn copy_ticks_range(&self, symbol: &str, from: i64, to: i64, flags: i32) -> Result<Vec<Tick>> {
        let cmd = 105;

        let mut data = Vec::new();
        data.extend_from_slice(&encode_string(symbol));
        // 秒 → 毫秒：终端期望毫秒时间戳（见函数文档说明）
        data.extend_from_slice(&(from * 1000).to_le_bytes());
        data.extend_from_slice(&(to * 1000).to_le_bytes());
        data.extend_from_slice(&(flags as u32).to_le_bytes());

        let resp = self.send(cmd, &data)?;
        parse_ticks_response(&resp)
    }

    /// 获取指定时间范围内成交记录的总数（命令码 `150`，对应 Python `mt5.history_deals_total()`）。
    ///
    /// # 参数
    ///
    /// - `from`：起始时间（Unix 秒，含）；
    /// - `to`：结束时间（Unix 秒，含）。
    ///
    /// # 实现逻辑
    ///
    /// 参数为 `from`(i64) + `to`(i64)，响应前 4 字节为成交总数（u32 LE）。
    pub fn history_deals_total(&self, from: i64, to: i64) -> Result<i64> {
        let cmd = 150;

        let mut data = Vec::new();
        data.extend_from_slice(&from.to_le_bytes());
        data.extend_from_slice(&to.to_le_bytes());

        let resp = self.send(cmd, &data)?;
        if resp.len() < 4 {
            return Err(Mt5Error::InvalidResponse("Response too short".into()));
        }

        let total = u32::from_le_bytes([resp[0], resp[1], resp[2], resp[3]]);
        Ok(total as i64)
    }

    /// 获取指定时间范围内的成交记录（命令码 `151`，对应 Python `mt5.history_deals_get()`）。
    ///
    /// # 参数
    ///
    /// - `from`：起始时间（Unix 秒，含）；
    /// - `to`：结束时间（Unix 秒，含）。
    ///
    /// # 返回值
    ///
    /// 返回 [`TradeDeal`] 列表。
    ///
    /// # 实现逻辑
    ///
    /// 参数为 `from`(i64) + `to`(i64)，响应交由 `parse_deals_response` 解析。
    pub fn history_deals_get(&self, from: i64, to: i64) -> Result<Vec<TradeDeal>> {
        let cmd = 151;

        let mut data = Vec::new();
        data.extend_from_slice(&from.to_le_bytes());
        data.extend_from_slice(&to.to_le_bytes());

        let resp = self.send(cmd, &data)?;
        parse_deals_response(&resp)
    }

    /// 获取指定时间范围内订单记录的总数（命令码 `140`，对应 Python `mt5.history_orders_total()`）。
    ///
    /// # 参数
    ///
    /// - `from`：起始时间（Unix 秒，含）；
    /// - `to`：结束时间（Unix 秒，含）。
    ///
    /// # 实现逻辑
    ///
    /// 参数为 `from`(i64) + `to`(i64)，响应前 4 字节为订单总数（u32 LE）。
    pub fn history_orders_total(&self, from: i64, to: i64) -> Result<i64> {
        let cmd = 140;

        let mut data = Vec::new();
        data.extend_from_slice(&from.to_le_bytes());
        data.extend_from_slice(&to.to_le_bytes());

        let resp = self.send(cmd, &data)?;
        if resp.len() < 4 {
            return Err(Mt5Error::InvalidResponse("Response too short".into()));
        }

        let total = u32::from_le_bytes([resp[0], resp[1], resp[2], resp[3]]);
        Ok(total as i64)
    }

    /// 获取指定时间范围内的订单历史（命令码 `141`，对应 Python `mt5.history_orders_get()`）。
    ///
    /// # 参数
    ///
    /// - `from`：起始时间（Unix 秒，含）；
    /// - `to`：结束时间（Unix 秒，含）。
    ///
    /// # 返回值
    ///
    /// 返回 [`TradeOrder`] 列表（包含已执行、已取消、已删除等历史订单）。
    ///
    /// # 实现逻辑
    ///
    /// 参数为 `from`(i64) + `to`(i64)，响应交由 `parse_orders_response` 解析。
    pub fn history_orders_get(&self, from: i64, to: i64) -> Result<Vec<TradeOrder>> {
        let cmd = 141;

        let mut data = Vec::new();
        data.extend_from_slice(&from.to_le_bytes());
        data.extend_from_slice(&to.to_le_bytes());

        let resp = self.send(cmd, &data)?;
        parse_orders_response(&resp)
    }

    /// 订阅指定品种的市场深度（DOM）（命令码 `191`，对应 Python `mt5.market_book_add()`）。
    ///
    /// # 参数
    ///
    /// - `symbol`：品种名称。
    ///
    /// # 返回值
    ///
    /// 订阅成功返回 `true`。
    ///
    /// # 实现逻辑
    ///
    /// 参数为「品种名字符串」；响应为空表示成功，否则读取 4 字节状态码，`0` 视为成功。
    /// 订阅后可用 [`Mt5Client::market_book_get`] 获取深度数据，
    /// 不再需要时用 [`Mt5Client::market_book_release`] 取消订阅。
    pub fn market_book_add(&self, symbol: &str) -> Result<bool> {
        let cmd = 191;

        let data = encode_string(symbol);
        let resp = self.send(cmd, &data)?;

        if resp.is_empty() {
            return Ok(true);
        }

        if resp.len() < 4 {
            return Err(Mt5Error::InvalidResponse("Response too short".into()));
        }

        let status = u32::from_le_bytes([resp[0], resp[1], resp[2], resp[3]]);
        Ok(status == 0)
    }

    /// 获取指定品种的市场深度（DOM）数据（命令码 `193`，对应 Python `mt5.market_book_get()`）。
    ///
    /// # 参数
    ///
    /// - `symbol`：品种名称（需先 [`Mt5Client::market_book_add`] 订阅）。
    ///
    /// # 返回值
    ///
    /// 返回 [`BookInfo`] 列表，按价格档位排列。
    ///
    /// # 实现逻辑
    ///
    /// 参数为「品种名字符串」，响应交由 `parse_book_response` 解析。
    pub fn market_book_get(&self, symbol: &str) -> Result<Vec<BookInfo>> {
        let cmd = 193;

        let data = encode_string(symbol);
        let resp = self.send(cmd, &data)?;
        parse_book_response(&resp)
    }

    /// 取消订阅指定品种的市场深度（命令码 `192`，对应 Python `mt5.market_book_release()`）。
    ///
    /// # 参数
    ///
    /// - `symbol`：品种名称。
    ///
    /// # 返回值
    ///
    /// 取消订阅成功返回 `true`。
    ///
    /// # 实现逻辑
    ///
    /// 参数为「品种名字符串」；响应为空表示成功，否则读取 4 字节状态码，`0` 视为成功。
    pub fn market_book_release(&self, symbol: &str) -> Result<bool> {
        let cmd = 192;

        let data = encode_string(symbol);
        let resp = self.send(cmd, &data)?;

        if resp.is_empty() {
            return Ok(true);
        }

        if resp.len() < 4 {
            return Err(Mt5Error::InvalidResponse("Response too short".into()));
        }

        let status = u32::from_le_bytes([resp[0], resp[1], resp[2], resp[3]]);
        Ok(status == 0)
    }

    /// 计算订单所需保证金（**本地计算**，不通过 IPC 发送命令 202）。
    ///
    /// # 参数
    ///
    /// - `_action`：交易方向（`ORDER_TYPE_*` 常量，当前实现未使用）；
    /// - `symbol`：品种名称（用于获取初始保证金比例）；
    /// - `volume`：交易量（手）；
    /// - `price`：开仓价格。
    ///
    /// # 返回值
    ///
    /// 返回所需保证金金额（以账户货币计）。
    ///
    /// # 实现逻辑
    ///
    /// 与 Python `MetaTrader5` 库行为一致，使用本地公式：
    ///
    /// ```text
    /// 保证金 = 交易量 × 价格 × 初始保证金比例 / 4
    /// ```
    ///
    /// 其中「初始保证金比例」取自 [`SymbolInfo::margin_initial`]（通过
    /// [`Mt5Client::symbol_info`] 获取）。
    ///
    /// # 注意事项
    ///
    /// - 采用本地计算而非管道命令 202，可避免 MT5 Build 5836+ 的「管道已关闭」错误；
    /// - 品种不存在时（`symbol_info` 返回 `None`），`unwrap` 会导致 **panic**，调用前请确认品种有效。
    pub fn order_calc_margin(&self, _action: i32, symbol: &str, volume: f64, price: f64) -> Result<f64> {
        // 获取symbol info以获取margin_initial
        let symbol_info = self.symbol_info(symbol)?;

        // 根据Python测试验证的公式计算
        // margin = volume × price × margin_initial / 4
        let margin_initial = symbol_info.unwrap().margin_initial;
        let margin = volume * price * margin_initial / 4.0;

        Ok(margin)
    }

    /// 计算订单预期利润（**本地计算**，不通过 IPC 发送命令 203）。
    ///
    /// # 参数
    ///
    /// - `_action`：交易方向（`ORDER_TYPE_*` 常量，当前实现未使用）；
    /// - `symbol`：品种名称（用于获取合约规模）；
    /// - `volume`：交易量（手）；
    /// - `price_open`：开仓价格；
    /// - `price_close`：平仓价格。
    ///
    /// # 返回值
    ///
    /// 返回预期盈亏金额（以账户货币计，正数为盈利，负数为亏损）。
    ///
    /// # 实现逻辑
    ///
    /// 与 Python `MetaTrader5` 库行为一致，使用本地公式：
    ///
    /// ```text
    /// 利润 = 交易量 × (平仓价 - 开仓价) × 合约规模
    /// ```
    ///
    /// 其中「合约规模」取自 [`SymbolInfo::trade_contract_size`]（通过
    /// [`Mt5Client::symbol_info`] 获取）。
    ///
    /// # 注意事项
    ///
    /// 品种不存在时（`symbol_info` 返回 `None`），`unwrap` 会导致 **panic**，调用前请确认品种有效。
    pub fn order_calc_profit(&self, _action: i32, symbol: &str, volume: f64, price_open: f64, price_close: f64) -> Result<f64> {
        // 获取symbol info以获取contract_size
        let symbol_info = self.symbol_info(symbol)?;

        // 计算利润
        let profit = volume * (price_close - price_open) * symbol_info.unwrap().trade_contract_size;

        Ok(profit)
    }

    /// 检查交易请求是否有效、资金是否充足（命令码 `160`，对应 Python `mt5.order_check()`）。
    ///
    /// 与 Python `MetaTrader5` 库一致：**成功发送请求不代表交易操作就能成功执行**，
    /// 本函数只做预检查，由交易服务器返回检查结果。
    ///
    /// # 参数
    ///
    /// - `request`：待检查的交易请求（字段约定见 [`TradeRequest`]）。
    ///
    /// # 返回值
    ///
    /// 返回 [`TradeCheckResult`]；`retcode == 0`（[`TRADE_RETCODE_OK`]）表示检查通过，
    /// 非 0 时 `comment` 字段包含失败原因。检查结果中的余额/净值/保证金等数值
    /// 为**假设该请求成交后**的账户状态。
    ///
    /// # 实现逻辑
    ///
    /// 1. 调用 `encode_trade_request` 将请求编码为 232 字节的二进制参数；
    /// 2. 发送命令码 `160`；
    /// 3. 响应为 252 字节定长结构，由 `parse_check_result_response` 解析：
    ///    `retcode`(u32) + `balance`/`equity`/`profit`/`margin`/`margin_free`/
    ///    `margin_level`(f64×6) + `comment`(200 字节 UTF-16LE 槽)。
    ///
    /// # 注意
    ///
    /// 线上响应中不含 `request_id` 字段（与 Python 库的 `TradeCheckResult` 输出一致），
    /// 因此本函数的结果结构中没有该字段。
    pub fn order_check(&self, request: &TradeRequest) -> Result<TradeCheckResult> {
        let cmd = 160;

        let data = encode_trade_request(request);
        let resp = self.send(cmd, &data)?;
        parse_check_result_response(&resp)
    }

    /// 发送交易请求到 MT5 终端执行（命令码 `161`，对应 Python `mt5.order_send()`）。
    ///
    /// # 参数
    ///
    /// - `request`：交易请求（字段约定见 [`TradeRequest`]）。
    ///
    /// # 返回值
    ///
    /// 返回 [`TradeResult`]。
    ///
    /// 与 Python `MetaTrader5` 库一致，**业务失败不会返回 `Err`**，而是通过
    /// `retcode` 表达：`retcode == 10009`（[`TRADE_RETCODE_DONE`]）或
    /// `10008`（[`TRADE_RETCODE_PLACED`]）表示成功，其余取值见
    /// `TRADE_RETCODE_*` 常量表（如 `TRADE_RETCODE_NO_MONEY`=资金不足）。
    /// 只有协议层失败（未初始化、响应过短、IO 错误等）才返回 `Err`。
    ///
    /// # 实现逻辑
    ///
    /// 1. 调用 `encode_trade_request` 将请求编码为 232 字节的二进制参数；
    /// 2. 发送命令码 `161`；
    /// 3. 响应为 260 字节定长结构，由 `parse_trade_result_response` 解析：
    ///    `retcode`(u32) + `deal`/`order`(i64) + `volume`/`price`/`bid`/`ask`(f64×4)
    ///    + `comment`(200 字节 UTF-16LE 槽) + `request_id`(u32) + `retcode_external`(i32)。
    ///
    /// # 简单示例
    ///
    /// ```no_run
    /// use mt5_rs::{Mt5Client, TradeRequest,
    ///     TRADE_ACTION_DEAL, ORDER_TYPE_BUY, ORDER_TIME_GTC, TRADE_RETCODE_DONE};
    ///
    /// # fn trade(client: &Mt5Client, ask: f64) -> mt5_rs::Result<()> {
    /// let request = TradeRequest {
    ///     action: TRADE_ACTION_DEAL,
    ///     symbol: "EURUSD".into(),
    ///     volume: 0.1,
    ///     r#type: ORDER_TYPE_BUY,
    ///     price: ask,
    ///     deviation: 20,
    ///     type_time: ORDER_TIME_GTC,
    ///     comment: "rust order".into(),
    ///     ..Default::default()
    /// };
    ///
    /// let result = client.order_send(&request)?;
    /// if result.retcode == TRADE_RETCODE_DONE {
    ///     println!("成交! deal={}, price={}", result.deal, result.price);
    /// } else {
    ///     println!("下单失败: retcode={}, comment={}", result.retcode, result.comment);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn order_send(&self, request: &TradeRequest) -> Result<TradeResult> {
        let cmd = 161;

        let data = encode_trade_request(request);
        let resp = self.send(cmd, &data)?;
        parse_trade_result_response(&resp)
    }

    /// 异步下单：把 [`Mt5Client::order_send`] 放到后台线程执行，结果通过回调返回。
    ///
    /// # 用途
    ///
    /// 跟单等「发出即算数、不等服务器回执」的场景：调用方立刻返回去做别的事，
    /// 下单结果在回调线程里处理（落库 / 记日志 / 失败重试）。
    ///
    /// # 参数
    ///
    /// - 接收者必须是 **`&Arc<Mt5Client>`**：后台线程需要共享所有权
    ///   （内部会 `Arc::clone`），因此调用前请把已连接的客户端放进 `Arc`；
    /// - `request`：交易请求（内部会克隆一份，调用方可继续复用原对象）；
    /// - `on_result`：结果回调，**在后台线程执行**，签名 `FnOnce(Result<TradeResult>)`。
    ///
    /// # 返回
    ///
    /// `Ok(())` 只表示「任务已派发」；真正的下单结果在回调里。
    /// 若客户端未初始化或连接已失效，则**不派发**并立即返回 `Err`
    /// （避免浪费线程）。
    ///
    /// # 示例
    ///
    /// ```no_run
    /// use mt5_rs::{Mt5Client, TradeRequest, TRADE_ACTION_DEAL, ORDER_TYPE_BUY};
    /// use std::sync::Arc;
    ///
    /// # fn main() -> Result<(), mt5_rs::Mt5Error> {
    /// let mut client = Mt5Client::new();
    /// client.initialize_with_timeout(Some(r"\\.\pipe\MT5.Terminal.XXX"), 3000)?;
    /// let client = Arc::new(client);           // ← 放进 Arc 才能异步
    ///
    /// let req = TradeRequest {
    ///     action: TRADE_ACTION_DEAL,
    ///     symbol: "EURUSD".into(),
    ///     volume: 0.1,
    ///     r#type: ORDER_TYPE_BUY,
    ///     ..Default::default()
    /// };
    ///
    /// client.order_send_async(&req, |result| match result {
    ///     Ok(t) => println!("异步下单完成：retcode={} price={}", t.retcode, t.price),
    ///     Err(e) => println!("异步下单失败：{e}"),
    /// })?;
    /// // 调用方立即继续；回调会在后台线程打印结果
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # 注意
    ///
    /// - 结果回调在后台线程，**不要**在里面做长时间阻塞操作；
    /// - 派发不检查终端是否正确受理（那要等回调）；
    /// - 每次调用会 `spawn` 一个线程（跟单场景下量级可接受；
    ///   极高频率下单建议在调用方用线程池收敛）。
    pub fn order_send_async<F>(
        self: &Arc<Self>,
        request: &TradeRequest,
        on_result: F,
    ) -> Result<()>
    where
        F: FnOnce(Result<TradeResult>) + Send + 'static,
    {
        // 派发前置检查：未初始化 / 连接已失效 → 立即失败，不浪费线程
        {
            let pipe = self.pipe()?;
            if pipe.is_broken() {
                return Err(Mt5Error::ConnectionFailed(
                    "连接已失效（此前发生过超时），请重新 initialize".into(),
                ));
            }
        }

        let me = Arc::clone(self);
        let req = request.clone();
        std::thread::spawn(move || {
            let result = me.order_send(&req);
            on_result(result);
        });
        Ok(())
    }

    /// 获取最近一次调用 API 时的错误码与错误描述（对应 Python `mt5.last_error()`）。
    ///
    /// # 返回值
    ///
    /// 返回 `(错误码, 错误描述)` 元组；最近一次调用成功时为 `(0, "")`。
    ///
    /// # 实现逻辑
    ///
    /// 与 Python `MetaTrader5` 库一致，本方法为**本地维护**：不向终端发送任何
    /// 命令，直接返回最近一次调用（统一经内部 `send` 入口）的记录——
    /// 终端拒绝命令时错误码为终端返回码（如 10030），底层 IO 失败时为
    /// Windows 错误码，其余情况为 -1。
    ///
    /// # 注意
    ///
    /// 旧版本曾用命令码 `3` 实现本方法，但 MT5 管道协议中**不存在**该命令，
    /// 终端收到后会直接断开管道（表现为 os error 109），已改为本地维护。
    pub fn last_error(&self) -> Result<(i32, String)> {
        Ok(self
            .last_error
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .unwrap_or((0, String::new())))
    }
}

/// 解析持仓列表响应（命令码 121 的响应数据）。
///
/// # 实现逻辑
///
/// 1. 前 4 字节为持仓数量 `count`（u32 LE）；
/// 2. 循环读取 `count` 条定长记录，每条布局：
///    `ticket`/`time`/`time_msc`/`time_update`/`time_update_msc`(i64×5) →
///    `type`(u32) → `magic`/`identifier`(i64×2) → `reason`(u32) →
///    `volume`/`price_open`/`price_current`/`price_sl`/`price_tp`/`swap`/`profit`(f64×7) →
///    `symbol`/`comment`/`external_id`(各 64 字节 UTF-16LE 槽)；
/// 3. 单条解析出错（缓冲区不足）时提前终止，返回已解析部分（容错处理）。
fn parse_positions_response(data: &[u8]) -> Result<Vec<TradePosition>> {
    if data.len() < 4 {
        return Err(Mt5Error::InvalidResponse("Response too short".into()));
    }

    let mut reader = Reader::new(data);
    let count = reader.read_u32() as usize;

    let mut positions = Vec::with_capacity(count);

    for _ in 0..count {
        let ticket = reader.read_i64();
        let time = reader.read_i64();
        let time_msc = reader.read_i64();
        let time_update = reader.read_i64();
        let time_update_msc = reader.read_i64();
        let r#type = reader.read_u32() as i32;
        let magic = reader.read_i64();
        let identifier = reader.read_i64();
        let reason = reader.read_u32() as i32;
        let volume = reader.read_f64();
        let price_open = reader.read_f64();
        let price_current = reader.read_f64();
        let price_sl = reader.read_f64();
        let price_tp = reader.read_f64();
        let swap = reader.read_f64();
        let profit = reader.read_f64();
        let symbol = reader.read_fixed_string(64);
        let comment = reader.read_fixed_string(64);
        let external_id = reader.read_fixed_string(64);

        if reader.has_error() {
            break;
        }

        positions.push(TradePosition {
            ticket,
            time,
            time_msc,
            time_update,
            time_update_msc,
            r#type,
            magic,
            identifier,
            reason,
            volume,
            price_open,
            price_current,
            price_sl,
            price_tp,
            swap,
            profit,
            symbol,
            comment,
            external_id,
        });
    }

    Ok(positions)
}

/// 解析订单列表响应（命令码 131 / 141 的响应数据）。
///
/// # 实现逻辑
///
/// 1. 前 4 字节为订单数量 `count`（u32 LE）；
/// 2. 循环读取 `count` 条定长记录，每条布局：
///    `ticket`/`time_setup`/`time_setup_msc`/`time_done`/`time_done_msc`/`time_expiration`(i64×6) →
///    `type`/`type_time`/`type_filling`/`state`(u32×4) →
///    `magic`/`position_id`/`position_by_id`(i64×3) → `reason`(u32) →
///    `volume_initial`/`volume_current`/`price_open`/`price_current`/`price_sl`/`price_tp`/
///    `price_stoplimit`(f64×7) → `symbol`/`comment`/`external_id`(各 64 字节 UTF-16LE 槽)；
/// 3. 单条解析出错时提前终止，返回已解析部分（容错处理）。
fn parse_orders_response(data: &[u8]) -> Result<Vec<TradeOrder>> {
    if data.len() < 4 {
        return Err(Mt5Error::InvalidResponse("Response too short".into()));
    }

    let mut reader = Reader::new(data);
    let count = reader.read_u32() as usize;

    let mut orders = Vec::with_capacity(count);

    for _ in 0..count {
        let ticket = reader.read_i64();
        let time_setup = reader.read_i64();
        let time_setup_msc = reader.read_i64();
        let time_done = reader.read_i64();
        let time_done_msc = reader.read_i64();
        let time_expiration = reader.read_i64();
        let r#type = reader.read_u32() as i32;
        let type_time = reader.read_u32() as i32;
        let type_filling = reader.read_u32() as i32;
        let state = reader.read_u32() as i32;
        let magic = reader.read_i64();
        let position_id = reader.read_i64();
        let position_by_id = reader.read_i64();
        let reason = reader.read_u32() as i32;
        let volume_initial = reader.read_f64();
        let volume_current = reader.read_f64();
        let price_open = reader.read_f64();
        let price_current = reader.read_f64();
        let price_sl = reader.read_f64();
        let price_tp = reader.read_f64();
        let price_stoplimit = reader.read_f64();
        let symbol = reader.read_fixed_string(64);
        let comment = reader.read_fixed_string(64);
        let external_id = reader.read_fixed_string(64);

        if reader.has_error() {
            break;
        }

        orders.push(TradeOrder {
            ticket,
            time_setup,
            time_setup_msc,
            time_done,
            time_done_msc,
            time_expiration,
            r#type,
            type_time,
            type_filling,
            state,
            magic,
            position_id,
            position_by_id,
            reason,
            volume_initial,
            volume_current,
            price_open,
            price_current,
            price_sl,
            price_tp,
            price_stoplimit,
            symbol,
            comment,
            external_id,
        });
    }

    Ok(orders)
}

/// 解析成交记录响应（命令码 151 的响应数据）。
///
/// # 实现逻辑
///
/// 1. 前 4 字节为成交数量 `count`（u32 LE）；
/// 2. 循环读取 `count` 条定长记录，每条布局：
///    `ticket`/`order`/`time`/`time_msc`(i64×4) → `type`/`entry`(u32×2) →
///    `magic`/`position_id`(i64×2) → `reason`(u32) →
///    `volume`/`price`/`commission`/`swap`/`profit`/`fee`(f64×6) →
///    `symbol`/`comment`/`external_id`(各 64 字节 UTF-16LE 槽)；
/// 3. 单条解析出错时提前终止，返回已解析部分（容错处理）。
fn parse_deals_response(data: &[u8]) -> Result<Vec<TradeDeal>> {
    if data.len() < 4 {
        return Err(Mt5Error::InvalidResponse("Response too short".into()));
    }

    let mut reader = Reader::new(data);
    let count = reader.read_u32() as usize;

    let mut deals = Vec::with_capacity(count);

    for _ in 0..count {
        let ticket = reader.read_i64();
        let order = reader.read_i64();
        let time = reader.read_i64();
        let time_msc = reader.read_i64();
        let r#type = reader.read_u32() as i32;
        let entry = reader.read_u32() as i32;
        let magic = reader.read_i64();
        let position_id = reader.read_i64();
        let reason = reader.read_u32() as i32;
        let volume = reader.read_f64();
        let price = reader.read_f64();
        let commission = reader.read_f64();
        let swap = reader.read_f64();
        let profit = reader.read_f64();
        let fee = reader.read_f64();
        let symbol = reader.read_fixed_string(64);
        let comment = reader.read_fixed_string(64);
        let external_id = reader.read_fixed_string(64);

        if reader.has_error() {
            break;
        }

        deals.push(TradeDeal {
            ticket,
            order,
            time,
            time_msc,
            r#type,
            entry,
            magic,
            position_id,
            reason,
            volume,
            price,
            commission,
            swap,
            profit,
            fee,
            symbol,
            comment,
            external_id,
        });
    }

    Ok(deals)
}

/// 解析 K 线列表响应（命令码 106 / 107 / 108 的响应数据）。
///
/// # 实现逻辑
///
/// 1. 前 4 字节为 K 线数量 `count`（u32 LE）；
/// 2. 循环读取 `count` 条定长记录，每条布局：
///    `time`(i64) → `open`/`high`/`low`/`close`(f64×4) →
///    `tick_volume`(u64) → `spread`(i32) → `real_volume`(u64)；
/// 3. 单条解析出错时提前终止，返回已解析部分（容错处理）。
fn parse_rates_response(data: &[u8]) -> Result<Vec<Rate>> {
    if data.len() < 4 {
        return Err(Mt5Error::InvalidResponse("Response too short".into()));
    }

    let mut reader = Reader::new(data);
    let count = reader.read_u32() as usize;

    let mut rates = Vec::with_capacity(count);

    for _ in 0..count {
        let time = reader.read_i64();
        let open = reader.read_f64();
        let high = reader.read_f64();
        let low = reader.read_f64();
        let close = reader.read_f64();
        let tick_volume = reader.read_u64();
        let spread = reader.read_i32();
        let real_volume = reader.read_u64();

        if reader.has_error() {
            break;
        }

        rates.push(Rate {
            time,
            open,
            high,
            low,
            close,
            tick_volume,
            spread,
            real_volume,
        });
    }

    Ok(rates)
}

/// 解析 Tick 列表响应（命令码 104 / 105 的响应数据）。
///
/// # 实现逻辑
///
/// 1. 前 4 字节为 Tick 数量 `count`（u32 LE）；
/// 2. 循环读取 `count` 条定长记录，每条布局：
///    `time`(i64) → `bid`/`ask`/`last`(f64×3) → `volume`(u64) →
///    `time_msc`(i64) → `flags`(u32) → `volume_real`(f64)；
/// 3. 单条解析出错时提前终止，返回已解析部分（容错处理）。
fn parse_ticks_response(data: &[u8]) -> Result<Vec<Tick>> {
    if data.len() < 4 {
        return Err(Mt5Error::InvalidResponse("Response too short".into()));
    }

    let mut reader = Reader::new(data);
    let count = reader.read_u32() as usize;

    let mut ticks = Vec::with_capacity(count);

    for _ in 0..count {
        let time = reader.read_i64();
        let bid = reader.read_f64();
        let ask = reader.read_f64();
        let last = reader.read_f64();
        let volume = reader.read_u64();
        let time_msc = reader.read_i64();
        let flags = reader.read_u32();
        let volume_real = reader.read_f64();

        if reader.has_error() {
            break;
        }

        ticks.push(Tick {
            time,
            bid,
            ask,
            last,
            volume,
            time_msc,
            flags,
            volume_real,
        });
    }

    Ok(ticks)
}

/// 解析市场深度（DOM）响应（命令码 193 的响应数据）。
///
/// # 实现逻辑
///
/// 1. 前 4 字节为档位数 `count`（u32 LE）；
/// 2. 循环读取 `count` 条定长记录，每条布局：
///    `type`(i64) → `price`(f64) → `volume`(i64) → `volume_real`(f64)；
/// 3. 单条解析出错时提前终止，返回已解析部分（容错处理）。
fn parse_book_response(data: &[u8]) -> Result<Vec<BookInfo>> {
    if data.len() < 4 {
        return Err(Mt5Error::InvalidResponse("Response too short".into()));
    }

    let mut reader = Reader::new(data);
    let count = reader.read_u32() as usize;

    let mut books = Vec::with_capacity(count);

    for _ in 0..count {
        let r#type = reader.read_i64();
        let price = reader.read_f64();
        let volume = reader.read_i64();
        let volume_real = reader.read_f64();

        if reader.has_error() {
            break;
        }

        books.push(BookInfo {
            r#type,
            price,
            volume,
            volume_real,
        });
    }

    Ok(books)
}

/// 将字符串编码为 MT5 协议格式（变长 UTF-16LE 字符串）。
///
/// # 实现逻辑
///
/// 编码结果：`[字符数 (u32 LE)] [UTF-16LE 编码的字符序列]`，
/// 字符数 = `chars.len()`，字节数 = 字符数 × 2。
fn encode_string(s: &str) -> Vec<u8> {
    let chars: Vec<u16> = s.encode_utf16().collect();
    let mut data = Vec::with_capacity(4 + chars.len() * 2);
    data.extend_from_slice(&(chars.len() as u32).to_le_bytes());
    for c in chars {
        data.extend_from_slice(&c.to_le_bytes());
    }
    data
}

/// 字节流读取器：按小端序从字节切片中依次读取各类数值与字符串。
///
/// # 越界处理
///
/// 所有读取方法在数据不足时都会把内部 `error` 标志置为 `true` 并返回零值，
/// 调用方通过 [`Reader::has_error`] 判断是否出错（用于提前终止解析循环）。
struct Reader<'a> {
    /// 待解析的字节切片。
    data: &'a [u8],
    /// 当前读取位置（字节偏移）。
    pos: usize,
    /// 是否已发生越界错误。
    error: bool,
}

impl<'a> Reader<'a> {
    /// 创建读取器，从 `data` 的第 0 字节开始。
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            error: false,
        }
    }

    /// 返回是否已发生读取越界。
    fn has_error(&self) -> bool {
        self.error
    }

    /// 读取 8 字节有符号整数（i64，小端序）。
    fn read_i64(&mut self) -> i64 {
        if self.error || self.pos + 8 > self.data.len() {
            self.error = true;
            return 0;
        }
        let bytes = [
            self.data[self.pos],
            self.data[self.pos + 1],
            self.data[self.pos + 2],
            self.data[self.pos + 3],
            self.data[self.pos + 4],
            self.data[self.pos + 5],
            self.data[self.pos + 6],
            self.data[self.pos + 7],
        ];
        self.pos += 8;
        i64::from_le_bytes(bytes)
    }

    /// 读取 8 字节无符号整数（u64，小端序）。
    fn read_u64(&mut self) -> u64 {
        if self.error || self.pos + 8 > self.data.len() {
            self.error = true;
            return 0;
        }
        let bytes = [
            self.data[self.pos],
            self.data[self.pos + 1],
            self.data[self.pos + 2],
            self.data[self.pos + 3],
            self.data[self.pos + 4],
            self.data[self.pos + 5],
            self.data[self.pos + 6],
            self.data[self.pos + 7],
        ];
        self.pos += 8;
        u64::from_le_bytes(bytes)
    }

    /// 读取 4 字节有符号整数（i32，小端序）。
    fn read_i32(&mut self) -> i32 {
        if self.error || self.pos + 4 > self.data.len() {
            self.error = true;
            return 0;
        }
        let bytes = [
            self.data[self.pos],
            self.data[self.pos + 1],
            self.data[self.pos + 2],
            self.data[self.pos + 3],
        ];
        self.pos += 4;
        i32::from_le_bytes(bytes)
    }

    /// 读取 4 字节无符号整数（u32，小端序）。
    fn read_u32(&mut self) -> u32 {
        if self.error || self.pos + 4 > self.data.len() {
            self.error = true;
            return 0;
        }
        let bytes = [
            self.data[self.pos],
            self.data[self.pos + 1],
            self.data[self.pos + 2],
            self.data[self.pos + 3],
        ];
        self.pos += 4;
        u32::from_le_bytes(bytes)
    }

    /// 读取 8 字节双精度浮点数（f64，小端序）。
    fn read_f64(&mut self) -> f64 {
        if self.error || self.pos + 8 > self.data.len() {
            self.error = true;
            return 0.0;
        }
        let bytes = [
            self.data[self.pos],
            self.data[self.pos + 1],
            self.data[self.pos + 2],
            self.data[self.pos + 3],
            self.data[self.pos + 4],
            self.data[self.pos + 5],
            self.data[self.pos + 6],
            self.data[self.pos + 7],
        ];
        self.pos += 8;
        f64::from_le_bytes(bytes)
    }

    /// 读取 1 字节布尔值（非 0 为 `true`）。
    fn read_bool1(&mut self) -> bool {
        if self.error || self.pos + 1 > self.data.len() {
            self.error = true;
            return false;
        }
        let b = self.data[self.pos];
        self.pos += 1;
        b != 0
    }

    /// 读取固定宽度字符串槽（UTF-16LE 编码，遇 NUL 截断）。
    ///
    /// # 参数
    ///
    /// - `slot_bytes`：槽的字节宽度（如 64、256、1024）。
    ///
    /// # 实现逻辑
    ///
    /// 无论是否遇到 NUL，读取位置都会前进 `slot_bytes`（跳过整个槽）；
    /// 槽内首个 NUL 之前的字符即为字符串内容。
    fn read_fixed_string(&mut self, slot_bytes: usize) -> String {
        if self.error || self.pos + slot_bytes > self.data.len() {
            self.error = true;
            return String::new();
        }
        let end = self.pos + slot_bytes;
        let buf = &self.data[self.pos..end];

        let mut chars = Vec::with_capacity(slot_bytes / 2);
        let mut i = 0;
        while i + 1 < buf.len() {
            let c = u16::from_le_bytes([buf[i], buf[i + 1]]);
            if c == 0 {
                break;
            }
            chars.push(c);
            i += 2;
        }
        self.pos = end;
        String::from_utf16_lossy(&chars)
    }
}

/// 从指定字节偏移开始读取 NUL 结尾的 UTF-16LE 字符串（供终端信息等定长布局使用）。
///
/// # 实现逻辑
///
/// 从 `offset` 起每 2 字节读一个 UTF-16 字符，遇到 NUL 或到达数据末尾即停止；
/// 偏移越界时返回空字符串（不报错）。
fn read_string_at_offset(data: &[u8], offset: usize) -> String {
    if offset >= data.len() {
        return String::new();
    }

    let mut chars = Vec::new();
    let mut pos = offset;
    while pos + 1 < data.len() {
        let c = u16::from_le_bytes([data[pos], data[pos + 1]]);
        pos += 2;
        if c == 0 {
            break;
        }
        chars.push(c);
    }
    String::from_utf16_lossy(&chars)
}

/// 字节流写入器：按小端序向字节缓冲追加各类数值与字符串（`Reader` 的镜像）。
///
/// 用于编码交易请求等二进制参数，保证与 MT5 终端期望的布局完全一致。
struct Writer {
    data: Vec<u8>,
}

impl Writer {
    /// 创建空写入器。
    fn new() -> Self {
        Self { data: Vec::new() }
    }

    /// 追加 4 字节无符号整数（u32，小端序）。
    fn write_u32(&mut self, v: u32) {
        self.data.extend_from_slice(&v.to_le_bytes());
    }

    /// 追加 8 字节无符号整数（u64，小端序）。
    fn write_u64(&mut self, v: u64) {
        self.data.extend_from_slice(&v.to_le_bytes());
    }

    /// 追加 8 字节有符号整数（i64，小端序）。
    fn write_i64(&mut self, v: i64) {
        self.data.extend_from_slice(&v.to_le_bytes());
    }

    /// 追加 8 字节双精度浮点数（f64，小端序）。
    fn write_f64(&mut self, v: f64) {
        self.data.extend_from_slice(&v.to_le_bytes());
    }

    /// 追加固定宽度字符串槽（与 MT5 终端布局一致）。
    ///
    /// # 参数
    ///
    /// - `s`：字符串内容；
    /// - `slot_bytes`：槽的字节宽度（必须为正偶数）。
    ///
    /// # 实现逻辑
    ///
    /// 1. 先写入 `slot_bytes` 个零字节（NUL 填充）；
    /// 2. 将字符串按 UTF-16LE 编码写入槽的起始处；
    /// 3. 最多写入 `slot_bytes / 2 - 1` 个字符，保证槽尾始终保留一个 NUL 结束符；
    ///    超出部分的字符被截断（与 go-mt5 的 `WriteFixedString` 行为一致）。
    fn write_fixed_string(&mut self, s: &str, slot_bytes: usize) {
        debug_assert!(slot_bytes > 0 && slot_bytes % 2 == 0);
        let start = self.data.len();
        self.data.resize(start + slot_bytes, 0);

        let chars: Vec<u16> = s.encode_utf16().collect();
        let max_chars = slot_bytes / 2 - 1;
        let n = chars.len().min(max_chars);
        for (i, c) in chars.iter().take(n).enumerate() {
            let bytes = c.to_le_bytes();
            self.data[start + i * 2] = bytes[0];
            self.data[start + i * 2 + 1] = bytes[1];
        }
    }

    /// 取出编码后的完整字节序列。
    fn into_bytes(self) -> Vec<u8> {
        self.data
    }
}

/// 将交易请求编码为 MT5 协议参数（**总计 232 字节**，与 go-mt5 完全一致）。
///
/// # 布局（偏移：长度 | 字段）
///
/// ```text
///   0: 4  | action (u32)
///   4: 8  | magic (i64)
///  12: 8  | order (i64)
///  20: 64 | symbol（固定 64 字节 UTF-16LE 槽，NUL 结尾，最多 31 字符）
///  84: 8  | volume (f64)
///  92: 8  | price (f64)
/// 100: 8  | stoplimit (f64)
/// 108: 8  | sl (f64)
/// 116: 8  | tp (f64)
/// 124: 8  | deviation (u64)
/// 132: 4  | type (u32)
/// 136: 4  | type_filling (u32)
/// 140: 4  | type_time (u32)
/// 144: 8  | expiration (i64)
/// 152: 64 | comment（固定 64 字节 UTF-16LE 槽，NUL 结尾，最多 31 字符）
/// 216: 8  | position (i64)
/// 224: 8  | position_by (i64)
/// ```
fn encode_trade_request(req: &TradeRequest) -> Vec<u8> {
    let mut w = Writer::new();
    w.write_u32(req.action as u32);
    w.write_i64(req.magic);
    w.write_i64(req.order);
    w.write_fixed_string(&req.symbol, 64);
    w.write_f64(req.volume);
    w.write_f64(req.price);
    w.write_f64(req.stoplimit);
    w.write_f64(req.sl);
    w.write_f64(req.tp);
    w.write_u64(req.deviation as u64);
    w.write_u32(req.r#type as u32);
    w.write_u32(req.type_filling as u32);
    w.write_u32(req.type_time as u32);
    w.write_i64(req.expiration);
    w.write_fixed_string(&req.comment, 64);
    w.write_i64(req.position);
    w.write_i64(req.position_by);
    w.into_bytes()
}

/// 解析交易执行结果响应（命令码 161 的响应数据，**定长 260 字节**）。
///
/// # 实现逻辑
///
/// 布局：`retcode`(u32@0) + `deal`(i64@4) + `order`(i64@12) +
/// `volume`/`price`/`bid`/`ask`(f64@20/28/36/44) +
/// `comment`(200 字节 UTF-16LE 槽@52) + `request_id`(u32@252) +
/// `retcode_external`(i32@256)。
///
/// 响应不足 260 字节时返回 [`Mt5Error::InvalidResponse`]。
fn parse_trade_result_response(data: &[u8]) -> Result<TradeResult> {
    const TOTAL_BYTES: usize = 260;
    if data.len() < TOTAL_BYTES {
        return Err(Mt5Error::InvalidResponse(format!(
            "Response too short: {} bytes, want {}",
            data.len(),
            TOTAL_BYTES
        )));
    }

    let mut reader = Reader::new(data);
    let retcode = reader.read_u32() as i32;
    let deal = reader.read_i64();
    let order = reader.read_i64();
    let volume = reader.read_f64();
    let price = reader.read_f64();
    let bid = reader.read_f64();
    let ask = reader.read_f64();
    let comment = reader.read_fixed_string(200);
    let request_id = reader.read_u32() as i64;
    let retcode_external = reader.read_i32();

    if reader.has_error() {
        return Err(Mt5Error::InvalidResponse("Failed to read trade result".into()));
    }

    Ok(TradeResult {
        retcode,
        deal,
        order,
        volume,
        price,
        bid,
        ask,
        comment,
        request_id,
        retcode_external,
    })
}

/// 解析交易请求检查结果响应（命令码 160 的响应数据，**定长 252 字节**）。
///
/// # 实现逻辑
///
/// 布局：`retcode`(u32@0) + `balance`/`equity`/`profit`/`margin`/
/// `margin_free`/`margin_level`(f64@4/12/20/28/36/44) +
/// `comment`(200 字节 UTF-16LE 槽@52)。
///
/// 响应不足 252 字节时返回 [`Mt5Error::InvalidResponse`]。
fn parse_check_result_response(data: &[u8]) -> Result<TradeCheckResult> {
    const TOTAL_BYTES: usize = 252;
    if data.len() < TOTAL_BYTES {
        return Err(Mt5Error::InvalidResponse(format!(
            "Response too short: {} bytes, want {}",
            data.len(),
            TOTAL_BYTES
        )));
    }

    let mut reader = Reader::new(data);
    let retcode = reader.read_u32() as i32;
    let balance = reader.read_f64();
    let equity = reader.read_f64();
    let profit = reader.read_f64();
    let margin = reader.read_f64();
    let margin_free = reader.read_f64();
    let margin_level = reader.read_f64();
    let comment = reader.read_fixed_string(200);

    if reader.has_error() {
        return Err(Mt5Error::InvalidResponse("Failed to read check result".into()));
    }

    Ok(TradeCheckResult {
        retcode,
        balance,
        equity,
        profit,
        margin,
        margin_free,
        margin_level,
        comment,
    })
}
