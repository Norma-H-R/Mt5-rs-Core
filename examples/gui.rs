//! mt5-rs 接口调试工具（GUI）。
//!
//! 类似微信公众平台/小程序接口调试页面：左侧为全部 35 项接口列表，
//! 右侧为中文参数表单（可手填）与执行按钮，下方为响应输出区。
//!
//! # 运行方式
//!
//! ```text
//! cargo run --example gui
//! ```
//!
//! # 使用说明
//!
//! 1. 先启动 MT5 终端并登录账户；
//! 2. 点击顶部「连接 MT5」（或左侧 `discover_mt5_pipe` / `initialize`）；
//! 3. 在左侧选择接口，右侧填写参数（时间类参数可直接点「现在」按钮填入当前秒），
//!    点击「执行测试」，结果输出到下方区域。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::time::{SystemTime, UNIX_EPOCH};

use eframe::egui;
use mt5_rs::{discover_mt5_pipe, Mt5Client, TradeRequest, TRADE_RETCODE_DONE, TRADE_RETCODE_PLACED};

// ============================================================================
// 参数模型
// ============================================================================

/// 参数类型：文本输入 或 勾选。
#[derive(Clone, Copy, PartialEq, Eq)]
enum ParamKind {
    Text,
    Bool,
}

/// 参数规格：唯一标识 + 中文标签 + 中文提示（占位文本）+ 默认值。
struct ParamSpec {
    key: &'static str,
    label: &'static str,
    hint: &'static str,
    kind: ParamKind,
    default: &'static str,
}

const fn ps(key: &'static str, label: &'static str, hint: &'static str, default: &'static str) -> ParamSpec {
    ParamSpec { key, label, hint, kind: ParamKind::Text, default }
}

const fn pb(key: &'static str, label: &'static str) -> ParamSpec {
    ParamSpec { key, label, hint: "", kind: ParamKind::Bool, default: "false" }
}

/// 参数当前值。
#[derive(Clone)]
enum ParamVal {
    Text(String),
    Bool(bool),
}

/// 接口定义：分组 + 函数名 + 中文说明 + 参数表。
struct ApiItem {
    group: &'static str,
    name: &'static str,
    desc: &'static str,
    params: &'static [ParamSpec],
}

// ============================================================================
// 接口参数表
// ============================================================================

/// order_check / order_send 共用的交易请求参数（17 项，对应 TradeRequest 字段）。
const TRADE_REQUEST_PARAMS: &[ParamSpec] = &[
    ps("action", "交易动作 action", "1=市价单 5=挂单 6=改止损止盈 7=修改挂单 8=删除挂单 10=反向平仓", "1"),
    ps("magic", "魔法数 magic", "EA 标识，用于识别自己的订单，默认 0", "0"),
    ps("order", "订单号 order", "修改/删除挂单时填写，其余为 0", "0"),
    ps("symbol", "品种 symbol", "如 EURUSD.s / AUDCAD.s", ""),
    ps("volume", "手数 volume", "如 0.01", "0.01"),
    ps("price", "价格 price", "市价单可填 0 或当前价；挂单填触发价", "0"),
    ps("stoplimit", "止损限价 stoplimit", "止损限价单的限价，默认 0", "0"),
    ps("sl", "止损价 sl", "默认 0（不设置）", "0"),
    ps("tp", "止盈价 tp", "默认 0（不设置）", "0"),
    ps("deviation", "最大滑点 deviation", "单位：点，默认 0", "0"),
    ps("type", "订单类型 type", "0=市价买 1=市价卖 2/3=限价单 4/5=止损单 6/7=止损限价单", "0"),
    ps("type_filling", "成交模式 type_filling", "0=FOK 1=IOC 2=RETURN 3=BOC（需品种支持）", "0"),
    ps("type_time", "有效期 type_time", "0=一直有效 1=当日 2=指定时间 3=指定日", "0"),
    ps("expiration", "到期时间 expiration", "Unix 秒，仅 type_time=2/3 时有效", "0"),
    ps("comment", "备注 comment", "最多 31 个字符", ""),
    ps("position", "持仓号 position", "平仓/改止损止盈时填目标持仓号", "0"),
    ps("position_by", "反向持仓号 position_by", "反向平仓时填写，其余为 0", "0"),
];

/// 35 项接口列表（顺序与 demo 一致）。
const APIS: &[ApiItem] = &[
    // ── 连接管理 ──
    ApiItem { group: "连接管理", name: "discover_mt5_pipe", desc: "自动发现 MT5 终端的命名管道（无终端时返回错误）", params: &[] },
    ApiItem { group: "连接管理", name: "initialize", desc: "初始化客户端并连接 MT5（管道名留空 = 自动发现）", params: &[
        ps("pipe_name", "管道名 pipe_name", "留空则自动发现，如 \\\\.\\pipe\\MT5.Terminal.XXXX", ""),
    ]},
    ApiItem { group: "连接管理", name: "shutdown", desc: "关闭与 MT5 终端的连接", params: &[] },

    // ── 账户与终端 ──
    ApiItem { group: "账户与终端", name: "account_info", desc: "获取账户信息（余额、净值、保证金、杠杆等）", params: &[] },
    ApiItem { group: "账户与终端", name: "terminal_info", desc: "获取终端信息（build、连接状态、路径等）", params: &[] },
    ApiItem { group: "账户与终端", name: "version", desc: "获取 MT5 版本信息", params: &[] },
    ApiItem { group: "账户与终端", name: "login", desc: "登录 MT5 账户（需账户密码服务器）", params: &[
        ps("login", "账户号码 login", "如 880899", ""),
        ps("password", "密码 password", "账户密码", ""),
        ps("server", "服务器 server", "如 DooTechnology-Demo", ""),
    ]},
    ApiItem { group: "账户与终端", name: "last_error", desc: "获取最近一次调用的错误码与描述（本地维护）", params: &[] },

    // ── 交易品种 ──
    ApiItem { group: "交易品种", name: "symbols_total", desc: "获取市场报价中品种总数", params: &[] },
    ApiItem { group: "交易品种", name: "symbols_get", desc: "获取全部品种信息（数量多，只展示前 3 个）", params: &[] },
    ApiItem { group: "交易品种", name: "symbol_info", desc: "获取单个品种的详细信息", params: &[
        ps("symbol", "品种 symbol", "如 EURUSD.s", ""),
    ]},
    ApiItem { group: "交易品种", name: "symbol_info_tick", desc: "获取品种最新报价 Tick", params: &[
        ps("symbol", "品种 symbol", "如 EURUSD.s", ""),
    ]},
    ApiItem { group: "交易品种", name: "symbol_select", desc: "在市场报价窗口中选中/取消选中品种", params: &[
        ps("symbol", "品种 symbol", "如 EURUSD.s", ""),
        pb("enable", "选中 enable（勾选 = 选中）"),
    ]},

    // ── 行情数据 ──
    ApiItem { group: "行情数据", name: "copy_rates_from_pos", desc: "从指定位置复制 K 线（0 = 最新一根）", params: &[
        ps("symbol", "品种 symbol", "如 EURUSD.s", ""),
        ps("timeframe", "周期 timeframe", "1=1分钟 5=5分钟 15=15分钟 30=30分钟 16385=1小时 16388=4小时 16408=日线 32769=周线 49153=月线", "1"),
        ps("start_pos", "起始位置 start_pos", "从最新 K 线往回数，0=最新", "0"),
        ps("count", "数量 count", "需要复制的 K 线根数", "10"),
    ]},
    ApiItem { group: "行情数据", name: "copy_rates_from", desc: "从指定时间开始复制 K 线", params: &[
        ps("symbol", "品种 symbol", "如 EURUSD.s", ""),
        ps("timeframe", "周期 timeframe", "见上（1/5/15/30/16385/16408 等）", "16408"),
        ps("date_from", "起始时间 date_from", "Unix 秒，可点「现在」填入", "0"),
        ps("count", "数量 count", "需要复制的 K 线根数", "30"),
    ]},
    ApiItem { group: "行情数据", name: "copy_rates_range", desc: "复制指定时间范围内的 K 线（to 建议留 10 分钟余量）", params: &[
        ps("symbol", "品种 symbol", "如 EURUSD.s", ""),
        ps("timeframe", "周期 timeframe", "见上（1/5/15/30/16385/16408 等）", "16385"),
        ps("date_from", "起始时间 date_from", "Unix 秒，可点「现在」填入", "0"),
        ps("date_to", "结束时间 date_to", "Unix 秒，建议填「现在-600」", "0"),
    ]},
    ApiItem { group: "行情数据", name: "copy_ticks_from", desc: "从指定时间开始复制 Tick（内部自动转毫秒）", params: &[
        ps("symbol", "品种 symbol", "如 EURUSD.s", ""),
        ps("from", "起始时间 from", "Unix 秒，可点「现在」填入", "0"),
        ps("count", "数量 count", "需要复制的 Tick 条数", "100"),
        ps("flags", "标志 flags", "-1=全部 1=仅买价 2=仅卖价", "-1"),
    ]},
    ApiItem { group: "行情数据", name: "copy_ticks_range", desc: "复制指定时间范围内的 Tick（内部自动转毫秒）", params: &[
        ps("symbol", "品种 symbol", "如 EURUSD.s", ""),
        ps("from", "起始时间 from", "Unix 秒，可点「现在」填入", "0"),
        ps("to", "结束时间 to", "Unix 秒，建议填「现在-600」", "0"),
        ps("flags", "标志 flags", "-1=全部 1=仅买价 2=仅卖价", "-1"),
    ]},

    // ── 持仓与订单 ──
    ApiItem { group: "持仓与订单", name: "positions_total", desc: "获取未平仓持仓总数", params: &[] },
    ApiItem { group: "持仓与订单", name: "positions_get", desc: "获取未平仓持仓（品种留空 = 全部）", params: &[
        ps("symbol", "品种过滤 symbol", "留空获取全部，或填 EURUSD.s", ""),
    ]},
    ApiItem { group: "持仓与订单", name: "orders_total", desc: "获取挂单总数", params: &[] },
    ApiItem { group: "持仓与订单", name: "orders_get", desc: "获取挂单列表（品种留空 = 全部）", params: &[
        ps("symbol", "品种过滤 symbol", "留空获取全部，或填 EURUSD.s", ""),
    ]},

    // ── 历史数据 ──
    ApiItem { group: "历史数据", name: "history_deals_total", desc: "获取时间范围内成交总数", params: &[
        ps("from", "起始时间 from", "Unix 秒，可点「现在」填入", "0"),
        ps("to", "结束时间 to", "Unix 秒，可点「现在」填入", "0"),
    ]},
    ApiItem { group: "历史数据", name: "history_deals_get", desc: "获取时间范围内成交记录", params: &[
        ps("from", "起始时间 from", "Unix 秒，可点「现在」填入", "0"),
        ps("to", "结束时间 to", "Unix 秒，可点「现在」填入", "0"),
    ]},
    ApiItem { group: "历史数据", name: "history_orders_total", desc: "获取时间范围内订单总数", params: &[
        ps("from", "起始时间 from", "Unix 秒，可点「现在」填入", "0"),
        ps("to", "结束时间 to", "Unix 秒，可点「现在」填入", "0"),
    ]},
    ApiItem { group: "历史数据", name: "history_orders_get", desc: "获取时间范围内订单历史", params: &[
        ps("from", "起始时间 from", "Unix 秒，可点「现在」填入", "0"),
        ps("to", "结束时间 to", "Unix 秒，可点「现在」填入", "0"),
    ]},

    // ── 市场深度 ──
    ApiItem { group: "市场深度", name: "market_book_add", desc: "订阅品种市场深度（DOM）", params: &[
        ps("symbol", "品种 symbol", "如 EURUSD.s", ""),
    ]},
    ApiItem { group: "市场深度", name: "market_book_get", desc: "获取市场深度档位（需先订阅；经纪商可能不提供）", params: &[
        ps("symbol", "品种 symbol", "如 EURUSD.s", ""),
    ]},
    ApiItem { group: "市场深度", name: "market_book_release", desc: "取消订阅市场深度", params: &[
        ps("symbol", "品种 symbol", "如 EURUSD.s", ""),
    ]},

    // ── 交易计算 ──
    ApiItem { group: "交易计算", name: "order_calc_margin", desc: "本地计算所需保证金（公式：手数×价格×初始保证金比例÷4）", params: &[
        ps("action", "交易动作 action", "1=市价单（当前实现未使用该参数）", "1"),
        ps("symbol", "品种 symbol", "如 EURUSD.s", ""),
        ps("volume", "手数 volume", "如 0.1", "0.1"),
        ps("price", "价格 price", "开仓价，如 0.987", "0"),
    ]},
    ApiItem { group: "交易计算", name: "order_calc_profit", desc: "本地计算预期利润（公式：手数×(平仓价-开仓价)×合约规模）", params: &[
        ps("action", "交易动作 action", "1=市价单（当前实现未使用该参数）", "1"),
        ps("symbol", "品种 symbol", "如 EURUSD.s", ""),
        ps("volume", "手数 volume", "如 0.1", "0.1"),
        ps("price_open", "开仓价 price_open", "如 0.987", "0"),
        ps("price_close", "平仓价 price_close", "如 0.997", "0"),
    ]},

    // ── 交易操作 ──
    ApiItem { group: "交易操作", name: "order_check", desc: "检查交易请求是否有效、资金是否充足（不会成交，安全）", params: TRADE_REQUEST_PARAMS },
    ApiItem { group: "交易操作", name: "order_send", desc: "发送交易请求到 MT5 执行（⚠ 会真实下单！业务失败通过 retcode 表达）", params: TRADE_REQUEST_PARAMS },

    // ── 高级接口 ──
    ApiItem { group: "高级接口", name: "send_raw_command", desc: "发送任意原始命令（协议调试用，注意：格式错误的请求会导致终端断开连接）", params: &[
        ps("cmd", "命令码 cmd", "如 190=账户信息 180=终端信息 170=品种信息", "190"),
        ps("data", "参数 data(十六进制)", "如 02000000 4C00 0000 ...（可留空）", ""),
    ]},
];

// ============================================================================
// 应用状态
// ============================================================================

struct GuiApp {
    /// 当前选中的接口索引。
    selected: usize,
    /// 每个接口的参数当前值（与 APIS 的 params 对齐）。
    values: Vec<Vec<ParamVal>>,
    /// 响应输出文本。
    output: String,
    /// 客户端连接（None = 未连接）。
    client: Option<Mt5Client>,
    /// 上次发现的管道名。
    pipe_name: String,
}

impl GuiApp {
    fn new() -> Self {
        let values = APIS
            .iter()
            .map(|api| {
                api.params
                    .iter()
                    .map(|p| match p.kind {
                        ParamKind::Text => ParamVal::Text(p.default.to_string()),
                        ParamKind::Bool => ParamVal::Bool(false),
                    })
                    .collect()
            })
            .collect();
        Self {
            selected: 0,
            values,
            output: String::new(),
            client: None,
            pipe_name: String::new(),
        }
    }

    // ---- 参数读取辅助 ----

    fn text(&self, idx: usize) -> String {
        match &self.values[self.selected][idx] {
            ParamVal::Text(s) => s.clone(),
            ParamVal::Bool(_) => String::new(),
        }
    }

    fn boolean(&self, idx: usize) -> bool {
        matches!(&self.values[self.selected][idx], ParamVal::Bool(true))
    }

    fn i64_param(&self, idx: usize, label: &str) -> std::result::Result<i64, String> {
        let owned = self.text(idx);
        let s = owned.trim();
        if s.is_empty() {
            return Err(format!("【{label}】不能为空"));
        }
        s.parse::<i64>().map_err(|_| format!("【{label}】不是有效的整数: {s}"))
    }

    fn i32_param(&self, idx: usize, label: &str) -> std::result::Result<i32, String> {
        self.i64_param(idx, label).map(|v| v as i32)
    }

    fn f64_param(&self, idx: usize, label: &str) -> std::result::Result<f64, String> {
        let owned = self.text(idx);
        let s = owned.trim();
        if s.is_empty() {
            return Err(format!("【{label}】不能为空"));
        }
        s.parse::<f64>().map_err(|_| format!("【{label}】不是有效的数字: {s}"))
    }

    fn opt_str(&self, idx: usize) -> Option<String> {
        let s = self.text(idx);
        if s.trim().is_empty() { None } else { Some(s.trim().to_string()) }
    }

    /// 输出一行响应（带时间戳）。
    fn out(&mut self, text: impl AsRef<str>) {
        self.output.push_str(text.as_ref());
        self.output.push('\n');
    }

    fn out_err(&mut self, e: &str) {
        self.output.push_str(&format!("❌ 调用失败: {e}\n"));
    }

    // ---- 连接管理 ----

    fn connect(&mut self, pipe_name: Option<String>) {
        self.out(format!("▶ 连接 MT5 ..."));
        let name = match pipe_name {
            Some(n) if !n.trim().is_empty() => n.trim().to_string(),
            _ => match std::panic::catch_unwind(discover_mt5_pipe) {
                Ok(n) => n,
                Err(_) => {
                    self.out("❌ 未找到 MT5 管道：请确认 MT5 终端已启动并登录账户。");
                    return;
                }
            },
        };
        let mut client = Mt5Client::new();
        match client.initialize(Some(&name)) {
            Ok(()) => {
                self.pipe_name = name;
                self.client = Some(client);
                self.out(format!("✅ 连接成功，管道: {}", self.pipe_name));
            }
            Err(e) => self.out_err(&e.to_string()),
        }
    }

    // ---- 执行选中的接口 ----

    fn run_selected(&mut self) {
        let idx = self.selected;
        let api = &APIS[idx];
        self.out(format!("────────────────────────────────────────────"));
        self.out(format!("[{}] ▶ 调用 {}({})", now_str(), api.name, api.desc));
        match idx {
            0 => self.api_discover(),
            1 => self.api_initialize(),
            2 => self.api_shutdown(),
            3 => self.api_account_info(),
            4 => self.api_terminal_info(),
            5 => self.api_version(),
            6 => self.api_login(),
            7 => self.api_last_error(),
            8 => self.api_symbols_total(),
            9 => self.api_symbols_get(),
            10 => self.api_symbol_info(),
            11 => self.api_symbol_info_tick(),
            12 => self.api_symbol_select(),
            13 => self.api_copy_rates_from_pos(),
            14 => self.api_copy_rates_from(),
            15 => self.api_copy_rates_range(),
            16 => self.api_copy_ticks_from(),
            17 => self.api_copy_ticks_range(),
            18 => self.api_positions_total(),
            19 => self.api_positions_get(),
            20 => self.api_orders_total(),
            21 => self.api_orders_get(),
            22 => self.api_history_deals_total(),
            23 => self.api_history_deals_get(),
            24 => self.api_history_orders_total(),
            25 => self.api_history_orders_get(),
            26 => self.api_market_book_add(),
            27 => self.api_market_book_get(),
            28 => self.api_market_book_release(),
            29 => self.api_order_calc_margin(),
            30 => self.api_order_calc_profit(),
            31 => self.api_order_check(),
            32 => self.api_order_send(),
            33 => self.api_send_raw_command(),
            _ => self.out("未知接口"),
        }
        self.out("");
    }

    // ---- 各接口实现 ----

    fn api_discover(&mut self) {
        match std::panic::catch_unwind(discover_mt5_pipe) {
            Ok(name) => {
                self.pipe_name = name.clone();
                self.out(format!("✅ 发现管道: {name}"));
            }
            Err(_) => self.out("❌ 未找到 MT5 管道：请确认 MT5 终端已启动并登录账户。"),
        }
    }

    fn api_initialize(&mut self) {
        let name = self.opt_str(0);
        self.connect(name);
    }

    fn api_shutdown(&mut self) {
        self.client = None;
        self.out("✅ 连接已关闭（shutdown）。");
    }

    fn api_account_info(&mut self) {
        let Some(c) = &self.client else { return self.out("❌ 未连接：请先点击顶部「连接 MT5」"); };
        match c.account_info() {
            Ok(a) => {
                self.out(format!(
                    "✅ 账户信息\n  login={}\n  server={}\n  company={}\n  currency={}\n  balance={}\n  equity={}\n  margin={}\n  free_margin={}\n  margin_level={}\n  profit={}\n  leverage={}\n  trade_mode={}\n  trade_allowed={}\n  name={}",
                    a.login, a.server, a.company, a.currency, a.balance, a.equity, a.margin,
                    a.free_margin, a.margin_level, a.profit, a.leverage, a.trade_mode, a.trade_allowed, a.name
                ));
            }
            Err(e) => self.out_err(&e.to_string()),
        }
    }

    fn api_terminal_info(&mut self) {
        let Some(c) = &self.client else { return self.out("❌ 未连接：请先点击顶部「连接 MT5」"); };
        match c.terminal_info() {
            Ok(t) => {
                self.out(format!(
                    "✅ 终端信息\n  build={}\n  connected={}\n  trade_allowed={}\n  company={}\n  name={}\n  language={}\n  path={}\n  data_path={}\n  max_bars={}\n  code_page={}\n  ping_last={}",
                    t.build, t.connected, t.trade_allowed, t.company, t.name, t.language,
                    t.path, t.data_path, t.max_bars, t.code_page, t.ping_last
                ));
            }
            Err(e) => self.out_err(&e.to_string()),
        }
    }

    fn api_version(&mut self) {
        let Some(c) = &self.client else { return self.out("❌ 未连接：请先点击顶部「连接 MT5」"); };
        match c.version() {
            Ok(v) => self.out(format!("✅ 版本信息\n  version={}\n  build={}\n  build_date={}", v.version, v.build, v.build_date)),
            Err(e) => self.out_err(&e.to_string()),
        }
    }

    fn api_login(&mut self) {
        let Some(c) = &self.client else { return self.out("❌ 未连接：请先点击顶部「连接 MT5」"); };
        let login = match self.i64_param(0, "账户号码") { Ok(v) => v, Err(e) => return self.out(e) };
        let password = self.text(1);
        let server = self.text(2);
        match c.login(login, &password, &server) {
            Ok(()) => self.out("✅ 登录成功。"),
            Err(e) => self.out_err(&e.to_string()),
        }
    }

    fn api_last_error(&mut self) {
        let Some(c) = &self.client else { return self.out("❌ 未连接：请先点击顶部「连接 MT5」"); };
        match c.last_error() {
            Ok((code, msg)) => self.out(format!("✅ 最近一次错误\n  code={}\n  message={}", code, msg)),
            Err(e) => self.out_err(&e.to_string()),
        }
    }

    fn api_symbols_total(&mut self) {
        let Some(c) = &self.client else { return self.out("❌ 未连接：请先点击顶部「连接 MT5」"); };
        match c.symbols_total() {
            Ok(n) => self.out(format!("✅ 品种总数 = {n}")),
            Err(e) => self.out_err(&e.to_string()),
        }
    }

    fn api_symbols_get(&mut self) {
        let Some(c) = &self.client else { return self.out("❌ 未连接：请先点击顶部「连接 MT5」"); };
        match c.symbols_get() {
            Ok(list) => {
                self.out(format!("✅ 共 {} 个品种，前 3 个:", list.len()));
                for s in list.iter().take(3) {
                    self.out(format!("  {}  bid={}  ask={}  point={}", s.name, s.bid, s.ask, s.point));
                }
            }
            Err(e) => self.out_err(&e.to_string()),
        }
    }

    fn api_symbol_info(&mut self) {
        let Some(c) = &self.client else { return self.out("❌ 未连接：请先点击顶部「连接 MT5」"); };
        let symbol = self.text(0);
        match c.symbol_info(&symbol) {
            Ok(Some(i)) => {
                self.out(format!(
                    "✅ 品种信息: {}\n  digits={}  point={}\n  bid={}  ask={}  last={}\n  spread={}  spread_float={}\n  contract_size={}  tick_size={}  tick_value={}\n  volume_min={}  volume_max={}  volume_step={}\n  margin_initial={}  margin_maintenance={}\n  trade_mode={}  trade_calc_mode={}\n  trade_stops_level={}  trade_freeze_level={}\n  filling_mode={}  order_mode={}  expiration_mode={}\n  swap_long={}  swap_short={}  swap_mode={}\n  description={}  currency_base={}  currency_profit={}",
                    i.name, i.digits, i.point, i.bid, i.ask, i.last, i.spread, i.spread_float,
                    i.trade_contract_size, i.trade_tick_size, i.trade_tick_value,
                    i.volume_min, i.volume_max, i.volume_step,
                    i.margin_initial, i.margin_maintenance,
                    i.trade_mode, i.trade_calc_mode, i.trade_stops_level, i.trade_freeze_level,
                    i.filling_mode, i.order_mode, i.expiration_mode,
                    i.swap_long, i.swap_short, i.swap_mode, i.description, i.currency_base, i.currency_profit
                ));
            }
            Ok(None) => self.out(format!("❌ 品种不存在: {symbol}")),
            Err(e) => self.out_err(&e.to_string()),
        }
    }

    fn api_symbol_info_tick(&mut self) {
        let Some(c) = &self.client else { return self.out("❌ 未连接：请先点击顶部「连接 MT5」"); };
        let symbol = self.text(0);
        match c.symbol_info_tick(&symbol) {
            Ok(Some(t)) => self.out(format!(
                "✅ 最新报价: {symbol}\n  time={}  time_msc={}\n  bid={}  ask={}  last={}\n  volume={}  volume_real={}\n  flags={}",
                t.time, t.time_msc, t.bid, t.ask, t.last, t.volume, t.volume_real, t.flags
            )),
            Ok(None) => self.out(format!("❌ 无报价: {symbol}")),
            Err(e) => self.out_err(&e.to_string()),
        }
    }

    fn api_symbol_select(&mut self) {
        let Some(c) = &self.client else { return self.out("❌ 未连接：请先点击顶部「连接 MT5」"); };
        let symbol = self.text(0);
        let enable = self.boolean(1);
        match c.symbol_select(&symbol, enable) {
            Ok(true) => self.out(format!("✅ 品种 {} 已{}。", symbol, if enable { "选中" } else { "取消选中" })),
            Ok(false) => self.out(format!("❌ 品种 {} 操作失败（返回 false）。", symbol)),
            Err(e) => self.out_err(&e.to_string()),
        }
    }

    fn api_copy_rates_from_pos(&mut self) {
        let Some(c) = &self.client else { return self.out("❌ 未连接：请先点击顶部「连接 MT5」"); };
        let symbol = self.text(0);
        let tf = match self.i32_param(1, "周期") { Ok(v) => v, Err(e) => return self.out(e) };
        let pos = match self.i64_param(2, "起始位置") { Ok(v) => v, Err(e) => return self.out(e) };
        let count = match self.i32_param(3, "数量") { Ok(v) => v, Err(e) => return self.out(e) };
        match c.copy_rates_from_pos(&symbol, tf, pos, count) {
            Ok(rates) => {
                self.out(format!("✅ 共 {} 根 K 线，前 3 根:", rates.len()));
                for r in rates.iter().take(3) {
                    self.out(format!("  time={}  O={}  H={}  L={}  C={}  tick_vol={}  spread={}", r.time, r.open, r.high, r.low, r.close, r.tick_volume, r.spread));
                }
            }
            Err(e) => self.out_err(&e.to_string()),
        }
    }

    fn api_copy_rates_from(&mut self) {
        let Some(c) = &self.client else { return self.out("❌ 未连接：请先点击顶部「连接 MT5」"); };
        let symbol = self.text(0);
        let tf = match self.i32_param(1, "周期") { Ok(v) => v, Err(e) => return self.out(e) };
        let from = match self.i64_param(2, "起始时间") { Ok(v) => v, Err(e) => return self.out(e) };
        let count = match self.i32_param(3, "数量") { Ok(v) => v, Err(e) => return self.out(e) };
        match c.copy_rates_from(&symbol, tf, from, count) {
            Ok(rates) => self.out(format!("✅ 共 {} 根 K 线，首根 time={}", rates.len(), rates.first().map(|r| r.time).unwrap_or(0))),
            Err(e) => self.out_err(&e.to_string()),
        }
    }

    fn api_copy_rates_range(&mut self) {
        let Some(c) = &self.client else { return self.out("❌ 未连接：请先点击顶部「连接 MT5」"); };
        let symbol = self.text(0);
        let tf = match self.i32_param(1, "周期") { Ok(v) => v, Err(e) => return self.out(e) };
        let from = match self.i64_param(2, "起始时间") { Ok(v) => v, Err(e) => return self.out(e) };
        let to = match self.i64_param(3, "结束时间") { Ok(v) => v, Err(e) => return self.out(e) };
        match c.copy_rates_range(&symbol, tf, from, to) {
            Ok(rates) => self.out(format!("✅ 共 {} 根 K 线，首根 time={} 末根 time={}", rates.len(), rates.first().map(|r| r.time).unwrap_or(0), rates.last().map(|r| r.time).unwrap_or(0))),
            Err(e) => self.out_err(&e.to_string()),
        }
    }

    fn api_copy_ticks_from(&mut self) {
        let Some(c) = &self.client else { return self.out("❌ 未连接：请先点击顶部「连接 MT5」"); };
        let symbol = self.text(0);
        let from = match self.i64_param(1, "起始时间") { Ok(v) => v, Err(e) => return self.out(e) };
        let count = match self.i32_param(2, "数量") { Ok(v) => v, Err(e) => return self.out(e) };
        let flags = match self.i32_param(3, "标志") { Ok(v) => v, Err(e) => return self.out(e) };
        match c.copy_ticks_from(&symbol, from, count, flags) {
            Ok(ticks) => {
                self.out(format!("✅ 共 {} 条 Tick，前 3 条:", ticks.len()));
                for t in ticks.iter().take(3) {
                    self.out(format!("  time={}  bid={}  ask={}  last={}  volume_real={}", t.time, t.bid, t.ask, t.last, t.volume_real));
                }
            }
            Err(e) => self.out_err(&e.to_string()),
        }
    }

    fn api_copy_ticks_range(&mut self) {
        let Some(c) = &self.client else { return self.out("❌ 未连接：请先点击顶部「连接 MT5」"); };
        let symbol = self.text(0);
        let from = match self.i64_param(1, "起始时间") { Ok(v) => v, Err(e) => return self.out(e) };
        let to = match self.i64_param(2, "结束时间") { Ok(v) => v, Err(e) => return self.out(e) };
        let flags = match self.i32_param(3, "标志") { Ok(v) => v, Err(e) => return self.out(e) };
        match c.copy_ticks_range(&symbol, from, to, flags) {
            Ok(ticks) => self.out(format!("✅ 共 {} 条 Tick，首条 time={}", ticks.len(), ticks.first().map(|t| t.time).unwrap_or(0))),
            Err(e) => self.out_err(&e.to_string()),
        }
    }

    fn api_positions_total(&mut self) {
        let Some(c) = &self.client else { return self.out("❌ 未连接：请先点击顶部「连接 MT5」"); };
        match c.positions_total() {
            Ok(n) => self.out(format!("✅ 持仓总数 = {n}")),
            Err(e) => self.out_err(&e.to_string()),
        }
    }

    fn api_positions_get(&mut self) {
        let Some(c) = &self.client else { return self.out("❌ 未连接：请先点击顶部「连接 MT5」"); };
        let symbol = self.opt_str(0);
        match c.positions_get(symbol.as_deref()) {
            Ok(list) => {
                self.out(format!("✅ 共 {} 笔持仓，前 3 笔:", list.len()));
                for p in list.iter().take(3) {
                    self.out(format!("  ticket={}  {}  {}  {}手  open={}  current={}  sl={}  tp={}  profit={}", p.ticket, p.symbol, if p.r#type == 0 { "BUY" } else { "SELL" }, p.volume, p.price_open, p.price_current, p.price_sl, p.price_tp, p.profit));
                }
            }
            Err(e) => self.out_err(&e.to_string()),
        }
    }

    fn api_orders_total(&mut self) {
        let Some(c) = &self.client else { return self.out("❌ 未连接：请先点击顶部「连接 MT5」"); };
        match c.orders_total() {
            Ok(n) => self.out(format!("✅ 挂单总数 = {n}")),
            Err(e) => self.out_err(&e.to_string()),
        }
    }

    fn api_orders_get(&mut self) {
        let Some(c) = &self.client else { return self.out("❌ 未连接：请先点击顶部「连接 MT5」"); };
        let symbol = self.opt_str(0);
        match c.orders_get(symbol.as_deref()) {
            Ok(list) => {
                self.out(format!("✅ 共 {} 笔挂单，前 3 笔:", list.len()));
                for o in list.iter().take(3) {
                    self.out(format!("  ticket={}  {}  type={}  {}手  price={}  state={}", o.ticket, o.symbol, o.r#type, o.volume_initial, o.price_open, o.state));
                }
            }
            Err(e) => self.out_err(&e.to_string()),
        }
    }

    fn api_history_deals_total(&mut self) {
        let Some(c) = &self.client else { return self.out("❌ 未连接：请先点击顶部「连接 MT5」"); };
        let from = match self.i64_param(0, "起始时间") { Ok(v) => v, Err(e) => return self.out(e) };
        let to = match self.i64_param(1, "结束时间") { Ok(v) => v, Err(e) => return self.out(e) };
        match c.history_deals_total(from, to) {
            Ok(n) => self.out(format!("✅ 成交总数 = {n}")),
            Err(e) => self.out_err(&e.to_string()),
        }
    }

    fn api_history_deals_get(&mut self) {
        let Some(c) = &self.client else { return self.out("❌ 未连接：请先点击顶部「连接 MT5」"); };
        let from = match self.i64_param(0, "起始时间") { Ok(v) => v, Err(e) => return self.out(e) };
        let to = match self.i64_param(1, "结束时间") { Ok(v) => v, Err(e) => return self.out(e) };
        match c.history_deals_get(from, to) {
            Ok(list) => {
                self.out(format!("✅ 共 {} 笔成交，前 3 笔:", list.len()));
                for d in list.iter().take(3) {
                    let sym = if d.symbol.is_empty() { "(账户操作)".to_string() } else { d.symbol.clone() };
                    self.out(format!("  ticket={}  {}  type={}  entry={}  {}手  price={}  profit={}  commission={}  swap={}", d.ticket, sym, d.r#type, d.entry, d.volume, d.price, d.profit, d.commission, d.swap));
                }
            }
            Err(e) => self.out_err(&e.to_string()),
        }
    }

    fn api_history_orders_total(&mut self) {
        let Some(c) = &self.client else { return self.out("❌ 未连接：请先点击顶部「连接 MT5」"); };
        let from = match self.i64_param(0, "起始时间") { Ok(v) => v, Err(e) => return self.out(e) };
        let to = match self.i64_param(1, "结束时间") { Ok(v) => v, Err(e) => return self.out(e) };
        match c.history_orders_total(from, to) {
            Ok(n) => self.out(format!("✅ 订单总数 = {n}")),
            Err(e) => self.out_err(&e.to_string()),
        }
    }

    fn api_history_orders_get(&mut self) {
        let Some(c) = &self.client else { return self.out("❌ 未连接：请先点击顶部「连接 MT5」"); };
        let from = match self.i64_param(0, "起始时间") { Ok(v) => v, Err(e) => return self.out(e) };
        let to = match self.i64_param(1, "结束时间") { Ok(v) => v, Err(e) => return self.out(e) };
        match c.history_orders_get(from, to) {
            Ok(list) => {
                self.out(format!("✅ 共 {} 笔历史订单，前 3 笔:", list.len()));
                for o in list.iter().take(3) {
                    self.out(format!("  ticket={}  {}  type={}  state={}  {}手  price={}", o.ticket, o.symbol, o.r#type, o.state, o.volume_initial, o.price_open));
                }
            }
            Err(e) => self.out_err(&e.to_string()),
        }
    }

    fn api_market_book_add(&mut self) {
        let Some(c) = &self.client else { return self.out("❌ 未连接：请先点击顶部「连接 MT5」"); };
        let symbol = self.text(0);
        match c.market_book_add(&symbol) {
            Ok(true) => self.out(format!("✅ 已订阅市场深度: {symbol}")),
            Ok(false) => self.out(format!("❌ 订阅失败: {symbol}")),
            Err(e) => self.out_err(&e.to_string()),
        }
    }

    fn api_market_book_get(&mut self) {
        let Some(c) = &self.client else { return self.out("❌ 未连接：请先点击顶部「连接 MT5」"); };
        let symbol = self.text(0);
        match c.market_book_get(&symbol) {
            Ok(list) => {
                self.out(format!("✅ 共 {} 档（经纪商可能不提供深度数据），前 5 档:", list.len()));
                for b in list.iter().take(5) {
                    self.out(format!("  type={}  price={}  volume={}  volume_real={}", b.r#type, b.price, b.volume, b.volume_real));
                }
            }
            Err(e) => self.out_err(&e.to_string()),
        }
    }

    fn api_market_book_release(&mut self) {
        let Some(c) = &self.client else { return self.out("❌ 未连接：请先点击顶部「连接 MT5」"); };
        let symbol = self.text(0);
        match c.market_book_release(&symbol) {
            Ok(true) => self.out(format!("✅ 已取消订阅市场深度: {symbol}")),
            Ok(false) => self.out(format!("❌ 取消订阅失败: {symbol}")),
            Err(e) => self.out_err(&e.to_string()),
        }
    }

    fn api_order_calc_margin(&mut self) {
        let Some(c) = &self.client else { return self.out("❌ 未连接：请先点击顶部「连接 MT5」"); };
        let action = match self.i32_param(0, "交易动作") { Ok(v) => v, Err(e) => return self.out(e) };
        let symbol = self.text(1);
        let volume = match self.f64_param(2, "手数") { Ok(v) => v, Err(e) => return self.out(e) };
        let price = match self.f64_param(3, "价格") { Ok(v) => v, Err(e) => return self.out(e) };
        match c.order_calc_margin(action, &symbol, volume, price) {
            Ok(m) => self.out(format!("✅ 所需保证金 = {m}")),
            Err(e) => self.out_err(&e.to_string()),
        }
    }

    fn api_order_calc_profit(&mut self) {
        let Some(c) = &self.client else { return self.out("❌ 未连接：请先点击顶部「连接 MT5」"); };
        let action = match self.i32_param(0, "交易动作") { Ok(v) => v, Err(e) => return self.out(e) };
        let symbol = self.text(1);
        let volume = match self.f64_param(2, "手数") { Ok(v) => v, Err(e) => return self.out(e) };
        let open = match self.f64_param(3, "开仓价") { Ok(v) => v, Err(e) => return self.out(e) };
        let close = match self.f64_param(4, "平仓价") { Ok(v) => v, Err(e) => return self.out(e) };
        match c.order_calc_profit(action, &symbol, volume, open, close) {
            Ok(p) => self.out(format!("✅ 预期利润 = {p}")),
            Err(e) => self.out_err(&e.to_string()),
        }
    }

    /// 从参数表构造 TradeRequest（order_check / order_send 共用）。
    fn trade_request_from_params(&self, prefix: &str) -> std::result::Result<TradeRequest, String> {
        // 两个接口的参数表相同（TRADE_REQUEST_PARAMS），但索引从各自表单第 0 项开始
        let t = |i: usize| self.text(i);
        let i64p = |i: usize, label: &str| -> std::result::Result<i64, String> {
            let owned = t(i);
            let s = owned.trim();
            if s.is_empty() { Ok(0) } else { s.parse::<i64>().map_err(|_| format!("【{label}】不是有效的整数: {s}")) }
        };
        let f64p = |i: usize, label: &str| -> std::result::Result<f64, String> {
            let owned = t(i);
            let s = owned.trim();
            if s.is_empty() { Ok(0.0) } else { s.parse::<f64>().map_err(|_| format!("【{label}】不是有效的数字: {s}")) }
        };
        let _ = prefix;
        Ok(TradeRequest {
            action: i64p(0, "action")? as i32,
            magic: i64p(1, "magic")?,
            order: i64p(2, "order")?,
            symbol: t(3).trim().to_string(),
            volume: f64p(4, "volume")?,
            price: f64p(5, "price")?,
            stoplimit: f64p(6, "stoplimit")?,
            sl: f64p(7, "sl")?,
            tp: f64p(8, "tp")?,
            deviation: i64p(9, "deviation")?,
            r#type: i64p(10, "type")? as i32,
            type_filling: i64p(11, "type_filling")? as i32,
            type_time: i64p(12, "type_time")? as i32,
            expiration: i64p(13, "expiration")?,
            comment: t(14).trim().to_string(),
            position: i64p(15, "position")?,
            position_by: i64p(16, "position_by")?,
        })
    }

    fn api_order_check(&mut self) {
        let Some(c) = &self.client else { return self.out("❌ 未连接：请先点击顶部「连接 MT5」"); };
        let request = match self.trade_request_from_params("order_check") { Ok(r) => r, Err(e) => return self.out(e) };
        if request.symbol.is_empty() {
            return self.out("❌ 请填写品种 symbol。");
        }
        match c.order_check(&request) {
            Ok(r) => self.out(format!(
                "✅ 检查结果\n  retcode={}  ({})\n  balance={}  equity={}\n  profit={}  margin={}\n  margin_free={}  margin_level={}\n  comment={}",
                r.retcode,
                if r.retcode == 0 { "检查通过" } else { "未通过" },
                r.balance, r.equity, r.profit, r.margin, r.margin_free, r.margin_level, r.comment
            )),
            Err(e) => self.out_err(&e.to_string()),
        }
    }

    fn api_order_send(&mut self) {
        let Some(c) = &self.client else { return self.out("❌ 未连接：请先点击顶部「连接 MT5」"); };
        let request = match self.trade_request_from_params("order_send") { Ok(r) => r, Err(e) => return self.out(e) };
        if request.symbol.is_empty() {
            return self.out("❌ 请填写品种 symbol。");
        }
        match c.order_send(&request) {
            Ok(r) => self.out(format!(
                "✅ 发送结果\n  retcode={}  ({})\n  deal={}  order={}\n  volume={}  price={}\n  bid={}  ask={}\n  request_id={}  retcode_external={}\n  comment={}",
                r.retcode,
                if r.retcode == TRADE_RETCODE_DONE || r.retcode == TRADE_RETCODE_PLACED { "成功" } else { "业务拒绝（见 TRADE_RETCODE_* 常量）" },
                r.deal, r.order, r.volume, r.price, r.bid, r.ask, r.request_id, r.retcode_external, r.comment
            )),
            Err(e) => self.out_err(&e.to_string()),
        }
    }

    fn api_send_raw_command(&mut self) {
        let Some(c) = &self.client else { return self.out("❌ 未连接：请先点击顶部「连接 MT5」"); };
        let cmd = match self.i64_param(0, "命令码") { Ok(v) => v as u32, Err(e) => return self.out(e) };
        let hex_str = self.text(1);
        let data = match parse_hex(&hex_str) {
            Ok(d) => d,
            Err(e) => return self.out(format!("❌ 参数解析失败: {e}")),
        };
        match c.send_raw_command(cmd, &data) {
            Ok(resp) => {
                let preview: Vec<String> = resp.iter().take(32).map(|b| format!("{b:02X}")).collect();
                self.out(format!("✅ 命令 {cmd} 返回 {} 字节:\n  前 32 字节: {}", resp.len(), preview.join(" ")));
            }
            Err(e) => self.out_err(&e.to_string()),
        }
    }
}

/// 解析十六进制字符串为字节数组（忽略空白）。
fn parse_hex(s: &str) -> std::result::Result<Vec<u8>, String> {
    let clean: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    if clean.is_empty() {
        return Ok(Vec::new());
    }
    if clean.len() % 2 != 0 {
        return Err(format!("十六进制长度必须为偶数: {}", clean));
    }
    (0..clean.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&clean[i..i + 2], 16)
                .map_err(|_| format!("无效十六进制: {}", &clean[i..i + 2]))
        })
        .collect()
}

/// 当前 Unix 秒。
fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// 当前时间字符串（yyyy-MM-dd HH:mm:ss）。
fn now_str() -> String {
    let secs = now_secs();
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400);
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}", rem / 3600, (rem % 3600) / 60, rem % 60)
}

/// Unix 天数 → (年, 月, 日)，Howard Hinnant 算法。
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

// ============================================================================
// egui 界面
// ============================================================================

fn setup_fonts(ctx: &egui::Context) {
    // 加载中文字体（优先黑体 simhei.ttf，其次微软雅黑 msyh.ttc / 宋体 simsun.ttc）
    for path in [
        "C:\\Windows\\Fonts\\msyh.ttc",
        "C:\\Windows\\Fonts\\simhei.ttf",
        "C:\\Windows\\Fonts\\simsun.ttc",
        "C:\\Windows\\Fonts\\msyhbd.ttc",
    ] {
        if let Ok(bytes) = std::fs::read(path) {
            let mut fonts = egui::FontDefinitions::default();
            fonts
                .font_data
                .insert("chinese".to_owned(), egui::FontData::from_owned(bytes).into());
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                fonts
                    .families
                    .entry(family)
                    .or_default()
                    .push("chinese".to_owned());
            }
            ctx.set_fonts(fonts);
            return;
        }
    }
}

impl eframe::App for GuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // ── 顶部：连接状态栏 ──
        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.heading("mt5-rs 接口调试工具");
                ui.separator();
                let connected = self.client.is_some();
                if connected {
                    ui.colored_label(egui::Color32::from_rgb(0, 180, 80), "● 已连接");
                } else {
                    ui.colored_label(egui::Color32::from_rgb(220, 80, 60), "● 未连接");
                }
                if !self.pipe_name.is_empty() {
                    ui.label(format!("管道: {}", self.pipe_name));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(egui::Button::new("断开连接")).clicked() {
                        self.client = None;
                        self.out("✅ 已断开连接。");
                    }
                    if ui.add(egui::Button::new("连接 MT5").min_size(egui::vec2(90.0, 24.0))).clicked() {
                        self.connect(None);
                    }
                });
            });
            ui.add_space(4.0);
        });

        // ── 左侧：接口列表 ──
        egui::SidePanel::left("api_list")
            .resizable(true)
            .default_width(250.0)
            .show(ctx, |ui| {
                ui.add_space(6.0);
                ui.label(egui::RichText::new(format!("接口列表（{} 项）", APIS.len())).strong());
                ui.separator();
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    let mut last_group = "";
                    for (i, api) in APIS.iter().enumerate() {
                        if api.group != last_group {
                            last_group = api.group;
                            ui.add_space(6.0);
                            ui.label(egui::RichText::new(api.group).color(egui::Color32::from_rgb(0, 130, 200)).strong());
                        }
                        let label = format!("{:<24} {}", api.name, "▶");
                        if ui.selectable_label(self.selected == i, label).clicked() {
                            self.selected = i;
                        }
                    }
                });
            });

        // ── 中央：参数表单 + 响应输出 ──
        egui::CentralPanel::default().show(ctx, |ui| {
            let api = &APIS[self.selected];
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.heading(api.name);
                ui.label(egui::RichText::new(api.desc).weak());
            });
            ui.separator();

            // 参数表单
            if api.params.is_empty() {
                ui.label(egui::RichText::new("（本接口无需参数）").weak());
            } else {
                egui::ScrollArea::vertical()
                    .id_salt("params_scroll")
                    .max_height(ui.available_height() * 0.45)
                    .show(ui, |ui| {
                        egui::Grid::new("param_grid")
                            .num_columns(2)
                            .spacing([12.0, 8.0])
                            .show(ui, |ui| {
                                let vals = &mut self.values[self.selected];
                                for (i, spec) in api.params.iter().enumerate() {
                                    match &mut vals[i] {
                                        ParamVal::Text(s) => {
                                            ui.label(format!("{}：", spec.label));
                                            ui.horizontal(|ui| {
                                                ui.add(
                                                    egui::TextEdit::singleline(s)
                                                        .hint_text(spec.hint)
                                                        .desired_width(320.0),
                                                );
                                                // 时间类参数：提供「现在」快捷按钮
                                                let is_time = matches!(
                                                    spec.key,
                                                    "date_from" | "date_to" | "from" | "to" | "expiration"
                                                );
                                                if is_time && ui.small_button("现在").clicked() {
                                                    *s = now_secs().to_string();
                                                }
                                            });
                                        }
                                        ParamVal::Bool(b) => {
                                            ui.label(format!("{}：", spec.label));
                                            ui.checkbox(b, "");
                                        }
                                    }
                                    ui.end_row();
                                }
                            });
                    });
            }

            // 执行按钮
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui
                    .add(egui::Button::new(egui::RichText::new("▶ 执行测试").size(16.0)).min_size(egui::vec2(140.0, 34.0)))
                    .clicked()
                {
                    self.run_selected();
                }
                if ui.button("清空输出").clicked() {
                    self.output.clear();
                }
                ui.label(egui::RichText::new("时间参数可点「现在」填入当前 Unix 秒").weak());
            });
            ui.add_space(4.0);

            // 响应输出区
            egui::TopBottomPanel::bottom("output_panel")
                .resizable(true)
                .default_height(240.0)
                .show_inside(ui, |ui| {
                    ui.label(egui::RichText::new("响应：").strong());
                    egui::ScrollArea::vertical()
                        .id_salt("output_scroll")
                        .auto_shrink([false, false])
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut self.output)
                                    .font(egui::TextStyle::Monospace)
                                    .desired_rows(14)
                                    .interactive(false)
                                    .desired_width(f32::INFINITY),
                            );
                        });
                });

            // 输出区下方留白（避免被 bottom panel 覆盖）
            ui.add_space(ui.available_height().max(0.0));
        });
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1180.0, 780.0])
            .with_title("mt5-rs 接口调试工具"),
        ..Default::default()
    };
    eframe::run_native(
        "mt5-rs 接口调试工具",
        options,
        Box::new(|cc| {
            setup_fonts(&cc.egui_ctx);
            Ok(Box::new(GuiApp::new()))
        }),
    )
}
