//! 数据结构定义。
//!
//! 本模块定义了 mt5-rs 与 MetaTrader 5 终端交互时使用的全部数据结构，
//! 字段命名与 Python `MetaTrader5` 库（以及 MT5 官方 `MQL5` 结构体）保持一致，
//! 便于使用者对照官方文档。

/// 账户信息（对应 MQL5 的 `ACCOUNT_INFO` 结构体）。
///
/// 通过 [`crate::Mt5Client::account_info`] 获取，描述当前登录账户的完整状态，
/// 包括余额、净值、保证金占用、可用保证金、杠杆等核心交易参数。
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone)]
pub struct AccountInfo {
    /// 账户号码（登录账号），如 12345678。
    pub login: i64,
    /// 交易模式（`TRADE_MODE_*` 常量）：0=演示账户，1=竞争账户，2=真实账户。
    pub trade_mode: i64,
    /// 账户杠杆，如 100 表示 1:100。
    pub leverage: i64,
    /// 最多允许的挂单数量。
    pub limit_orders: i64,
    /// 止损/止盈模式（`SYMBOL_MARGIN_MODE_*` 常量）。
    pub margin_so_mode: i64,
    /// 是否允许当前账户进行交易。
    pub trade_allowed: bool,
    /// 是否允许智能交易系统（EA）进行交易。
    pub trade_expert: bool,
    /// 保证金计算模式（`SYMBOL_MARGIN_MODE_*` 常量）。
    pub margin_mode: i64,
    /// 账户货币小数点后的位数（如美元账户为 2）。
    pub currency_digits: i64,
    /// 是否强制 FIFO（先进先出）平仓规则（美国账户监管要求）。
    pub fifo_close: bool,
    /// 账户余额（以账户货币计）。
    pub balance: f64,
    /// 信用额度（经纪商提供的信用资金）。
    pub credit: f64,
    /// 当前浮动盈亏。
    pub profit: f64,
    /// 净值 = 余额 + 信用额度 + 浮动盈亏。
    pub equity: f64,
    /// 当前占用的保证金总额。
    pub margin: f64,
    /// 可用（自由）保证金 = 净值 - 占用保证金。
    pub free_margin: f64,
    /// 保证金水平（百分比）= 净值 / 占用保证金 × 100。
    pub margin_level: f64,
    /// 触发追加保证金通知的保证金水平。
    pub margin_so_call: f64,
    /// 触发强制平仓的保证金水平。
    pub margin_so_so: f64,
    /// 开仓所需的初始保证金。
    pub margin_initial: f64,
    /// 维持仓位所需的最低保证金。
    pub margin_maintenance: f64,
    /// 资产总额。
    pub assets: f64,
    /// 负债总额。
    pub liabilities: f64,
    /// 被冻结的佣金（如未结订单占用）。
    pub commission_blocked: f64,
    /// 账户持有人姓名。
    pub name: String,
    /// 账户所属交易服务器名称。
    pub server: String,
    /// 账户货币（如 USD、EUR）。
    pub currency: String,
    /// 经纪公司名称。
    pub company: String,
}

/// 终端信息（对应 MQL5 的 `TERMINAL_INFO` 结构体）。
///
/// 通过 [`crate::Mt5Client::terminal_info`] 获取，描述 MT5 终端程序的运行状态
/// 与安装路径等信息。
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone)]
pub struct TerminalInfo {
    /// 是否登录了 MQL5 社区账户。
    pub community_account: bool,
    /// 是否已连接到 MQL5 社区。
    pub community_connection: bool,
    /// 是否已连接到交易服务器。
    pub connected: bool,
    /// 是否允许导入 DLL（动态链接库）。
    pub dlls_allowed: bool,
    /// 是否允许交易。
    pub trade_allowed: bool,
    /// 是否通过 API 禁用了自动交易。
    pub trade_api_disabled: bool,
    /// 是否启用了邮件发送功能。
    pub email_enabled: bool,
    /// 是否启用了 FTP 上传功能。
    pub ftp_enabled: bool,
    /// 是否启用了推送通知功能。
    pub notifications_enabled: bool,
    /// 是否启用了 MQL5 社区消息队列（MQID）。
    pub mqid: bool,
    /// 终端版本号（build 号）。
    pub build: i64,
    /// 图表中最多允许显示的 K 线数量。
    pub max_bars: i64,
    /// 终端使用的代码页。
    pub code_page: i64,
    /// 上次与交易服务器通信的 ping 值（毫秒）。
    pub ping_last: i64,
    /// 社区账户余额。
    pub community_balance: f64,
    /// 交易服务器数据重传计数。
    pub retransmission: f64,
    /// 经纪公司名称。
    pub company: String,
    /// 终端名称。
    pub name: String,
    /// 终端界面语言。
    pub language: String,
    /// 终端可执行文件所在目录。
    pub path: String,
    /// 终端数据目录。
    pub data_path: String,
    /// 所有终端实例共享的数据目录。
    pub common_data_path: String,
}

/// MT5 版本信息。
///
/// 通过 [`crate::Mt5Client::version`] 获取，注意当前实现中 `version` 与 `build`
/// 均取自终端 build 号，`build_date` 由公司名与终端名拼接而成。
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone)]
pub struct VersionInfo {
    /// 终端版本号。
    pub version: i32,
    /// 终端 build 号。
    pub build: i32,
    /// 版本日期描述（当前实现为 `"公司名 (终端名)"` 格式的字符串）。
    pub build_date: String,
}

/// 交易品种信息（对应 MQL5 的 `SYMBOL_INFO` 结构体）。
///
/// 通过 [`crate::Mt5Client::symbol_info`] 或 [`crate::Mt5Client::symbols_get`] 获取，
/// 描述一个交易品种（如 EURUSD）的合约参数、行情快照、会话与保证金规则等全部属性。
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone)]
pub struct SymbolInfo {
    /// 是否为自定义品种（由用户/经纪商自行定义）。
    pub custom: bool,
    /// 图表显示模式（`CHART_MODE_*` 常量）。
    pub chart_mode: i64,
    /// 品种是否在“市场报价”窗口中可见/被选中。
    pub select: bool,
    /// 品种是否可见。
    pub visible: bool,
    /// 当前交易会话中的成交笔数。
    pub session_deals: i64,
    /// 当前交易会话中的买单数量。
    pub session_buy_orders: i64,
    /// 当前交易会话中的卖单数量。
    pub session_sell_orders: i64,
    /// 当前交易会话中的成交量。
    pub volume: i64,
    /// 当前交易会话中的最高成交量。
    pub volume_high: i64,
    /// 当前交易会话中的最低成交量。
    pub volume_low: i64,
    /// 最后行情的时间戳（Unix 秒）。
    pub time: i64,
    /// 价格小数点后的位数。
    pub digits: i64,
    /// 当前点差（以点为单位）。
    pub spread: i64,
    /// 点差是否为浮动点差。
    pub spread_float: bool,
    /// 市场深度（DOM）的最大档位数。
    pub ticks_book_depth: i64,
    /// 保证金计算模式（`SYMBOL_TRADE_CALC_MODE_*` 常量，如 0=外汇、1=差价合约等）。
    pub trade_calc_mode: i64,
    /// 交易模式（`SYMBOL_TRADE_MODE_*` 常量，如 0=禁止交易、1=允许市价等）。
    pub trade_mode: i64,
    /// 品种交易开始时间（Unix 秒，0 表示无限制）。
    pub start_time: i64,
    /// 品种交易结束时间（Unix 秒，0 表示无限制）。
    pub expiration_time: i64,
    /// 止损/止盈价与市价的最小距离（点）。
    pub trade_stops_level: i64,
    /// 冻结交易的价位（点）。
    pub trade_freeze_level: i64,
    /// 交易执行模式（`SYMBOL_TRADE_EXECUTION_*` 常量）。
    pub trade_exe_mode: i64,
    /// 隔夜利息（掉期）计算模式（`SYMBOL_SWAP_MODE_*` 常量）。
    pub swap_mode: i64,
    /// 三天掉期的计算方式（`SYMBOL_SWAP_3DAYS_*` 常量）。
    pub swap_rollover3days: i64,
    /// 对冲保证金是否按腿（leg）分别计算。
    pub margin_hedged_use_leg: bool,
    /// 期权/期货到期模式（`SYMBOL_EXPIRATION_MODE_*` 常量）。
    pub expiration_mode: i64,
    /// 订单成交模式（`SYMBOL_FILLING_MODE_*` 常量，如 FOK、IOC、RETURN）。
    pub filling_mode: i64,
    /// 订单类型模式（`SYMBOL_ORDER_MODE_*` 常量）。
    pub order_mode: i64,
    /// 订单有效期模式（`SYMBOL_ORDER_GTC_MODE_*` 常量）。
    pub order_gtc_mode: i64,
    /// 期权模式（`SYMBOL_OPTION_MODE_*` 常量）。
    pub option_mode: i64,
    /// 期权权利（看涨/看跌，`SYMBOL_OPTION_RIGHT_*` 常量）。
    pub option_right: i64,
    /// 当前买价（Bid）。
    pub bid: f64,
    /// 本日最高买价。
    pub bidhigh: f64,
    /// 本日最低买价。
    pub bidlow: f64,
    /// 当前卖价（Ask）。
    pub ask: f64,
    /// 本日最高卖价。
    pub askhigh: f64,
    /// 本日最低卖价。
    pub asklow: f64,
    /// 最后成交价。
    pub last: f64,
    /// 本日最高最后成交价。
    pub lasthigh: f64,
    /// 本日最低最后成交价。
    pub lastlow: f64,
    /// 当前实际成交量。
    pub volume_real: f64,
    /// 本日最高实际成交量。
    pub volumehigh_real: f64,
    /// 本日最低实际成交量。
    pub volumelow_real: f64,
    /// 期权行权价。
    pub option_strike: f64,
    /// 价格最小变动单位（点值，如 0.00001）。
    pub point: f64,
    /// 每 tick 价格变动对应的盈亏金额（按账户货币计）。
    pub trade_tick_value: f64,
    /// 盈利方向每 tick 的盈亏金额。
    pub trade_tick_value_profit: f64,
    /// 亏损方向每 tick 的盈亏金额。
    pub trade_tick_value_loss: f64,
    /// tick 大小（价格跳动的最小间隔）。
    pub trade_tick_size: f64,
    /// 合约规模（1 手对应的标的数量）。
    pub trade_contract_size: f64,
    /// 应计利息（用于债券类品种）。
    pub trade_accrued_interest: f64,
    /// 票面价值（用于债券类品种）。
    pub trade_face_value: f64,
    /// 流动性比率。
    pub trade_liquidity_rate: f64,
    /// 最小交易量（手）。
    pub volume_min: f64,
    /// 最大交易量（手）。
    pub volume_max: f64,
    /// 交易量步进（手）。
    pub volume_step: f64,
    /// 期权限仓交易量。
    pub volume_limit: f64,
    /// 多头持仓的隔夜利息（每手）。
    pub swap_long: f64,
    /// 空头持仓的隔夜利息（每手）。
    pub swap_short: f64,
    /// 开仓初始保证金。
    pub margin_initial: f64,
    /// 维持保证金。
    pub margin_maintenance: f64,
    /// 当前交易会话的成交量。
    pub session_volume: f64,
    /// 当前交易会话的成交额。
    pub session_turnover: f64,
    /// 当前交易会话的利息。
    pub session_interest: f64,
    /// 当前交易会话的买单成交量。
    pub session_buy_orders_volume: f64,
    /// 当前交易会话的卖单成交量。
    pub session_sell_orders_volume: f64,
    /// 当前交易会话的开盘价。
    pub session_open: f64,
    /// 当前交易会话的收盘价。
    pub session_close: f64,
    /// 当前交易会话的加权平均价（AW）。
    pub session_aw: f64,
    /// 当前交易会话的结算价。
    pub session_price_settlement: f64,
    /// 当前交易会话的最低限价。
    pub session_price_limit_min: f64,
    /// 当前交易会话的最高限价。
    pub session_price_limit_max: f64,
    /// 完全对冲持仓所需的保证金。
    pub margin_hedged: f64,
    /// 价格变动（相对上一时段的涨跌）。
    pub price_change: f64,
    /// 价格波动率。
    pub price_volatility: f64,
    /// 理论价格（用于期权定价模型）。
    pub price_theoretical: f64,
    /// 期权希腊字母：Delta（标的价格变动对期权价格的影响）。
    pub price_greeks_delta: f64,
    /// 期权希腊字母：Theta（时间衰减）。
    pub price_greeks_theta: f64,
    /// 期权希腊字母：Gamma（Delta 的变化率）。
    pub price_greeks_gamma: f64,
    /// 期权希腊字母：Vega（波动率敏感度）。
    pub price_greeks_vega: f64,
    /// 期权希腊字母：Rho（利率敏感度）。
    pub price_greeks_rho: f64,
    /// 期权希腊字母：Omega（杠杆率）。
    pub price_greeks_omega: f64,
    /// 价格敏感度。
    pub price_sensitivity: f64,
    /// 标的资产（basis）描述。
    pub basis: String,
    /// 品种类别。
    pub category: String,
    /// 基础货币（如 EURUSD 的 EUR）。
    pub currency_base: String,
    /// 盈利货币（计算盈亏所用的货币）。
    pub currency_profit: String,
    /// 保证金货币。
    pub currency_margin: String,
    /// 提供该品种的银行名称。
    pub bank: String,
    /// 品种描述。
    pub description: String,
    /// 交易所名称。
    pub exchange: String,
    /// 价格计算公式（用于指数等计算型品种）。
    pub formula: String,
    /// ISIN 代码（国际证券识别码）。
    pub isin: String,
    /// 品种名称（如 "EURUSD"）。
    pub name: String,
    /// 品种相关网页。
    pub page: String,
    /// 品种在终端中的路径（用于自定义品种分组）。
    pub path: String,
}

/// 一笔实时报价（Tick，对应 MQL5 的 `MqlTick` 结构体）。
///
/// 通过 [`crate::Mt5Client::symbol_info_tick`]、[`crate::Mt5Client::copy_ticks_from`]
/// 或 [`crate::Mt5Client::copy_ticks_range`] 获取。
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone)]
pub struct Tick {
    /// 报价时间（Unix 秒）。
    pub time: i64,
    /// 买价（Bid）。
    pub bid: f64,
    /// 卖价（Ask）。
    pub ask: f64,
    /// 最后成交价（Last）。
    pub last: f64,
    /// 成交量（以手为单位，整数部分）。
    pub volume: u64,
    /// 报价时间（毫秒精度）。
    pub time_msc: i64,
    /// tick 标志位（`TICK_FLAG_*` 位掩码，标明哪些字段有效）。
    pub flags: u32,
    /// 实际成交量（双精度）。
    pub volume_real: f64,
}

/// 一根 K 线（对应 MQL5 的 `MqlRates` 结构体）。
///
/// 通过 [`crate::Mt5Client::copy_rates_from_pos`]、[`crate::Mt5Client::copy_rates_from`]
/// 或 [`crate::Mt5Client::copy_rates_range`] 获取。
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone)]
pub struct Rate {
    /// K 线时间（Unix 秒，为该周期开盘时刻）。
    pub time: i64,
    /// 开盘价。
    pub open: f64,
    /// 最高价。
    pub high: f64,
    /// 最低价。
    pub low: f64,
    /// 收盘价。
    pub close: f64,
    /// 该 K 线内的 tick 成交量。
    pub tick_volume: u64,
    /// 点差（以点为单位）。
    pub spread: i32,
    /// 真实成交量（从交易所获取）。
    pub real_volume: u64,
}

/// 一笔未平仓持仓（对应 MQL5 的 `POSITION_*` 属性集）。
///
/// 通过 [`crate::Mt5Client::positions_get`] 获取。
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone)]
pub struct TradePosition {
    /// 持仓票据号（唯一标识）。
    pub ticket: i64,
    /// 开仓时间（Unix 秒）。
    pub time: i64,
    /// 开仓时间（毫秒精度）。
    pub time_msc: i64,
    /// 持仓最近一次变动时间（Unix 秒）。
    pub time_update: i64,
    /// 持仓最近一次变动时间（毫秒精度）。
    pub time_update_msc: i64,
    /// 持仓类型（`POSITION_TYPE_*` 常量：0=买入，1=卖出）。
    pub r#type: i32,
    /// 魔法数（EA 用于标识自己创建的订单）。
    pub magic: i64,
    /// 持仓标识符（区别于票据号，平仓后不变化）。
    pub identifier: i64,
    /// 开仓原因（`POSITION_REASON_*` 常量）。
    pub reason: i32,
    /// 持仓量（手）。
    pub volume: f64,
    /// 开仓价。
    pub price_open: f64,
    /// 当前市价。
    pub price_current: f64,
    /// 止损价（0 表示未设置）。
    pub price_sl: f64,
    /// 止盈价（0 表示未设置）。
    pub price_tp: f64,
    /// 累计隔夜利息（掉期）。
    pub swap: f64,
    /// 浮动盈亏。
    pub profit: f64,
    /// 交易品种名称。
    pub symbol: String,
    /// 持仓备注。
    pub comment: String,
    /// 外部系统标识符。
    pub external_id: String,
}

/// 一笔订单（挂单或已执行订单，对应 MQL5 的 `ORDER_*` 属性集）。
///
/// 通过 [`crate::Mt5Client::orders_get`] 或 [`crate::Mt5Client::history_orders_get`] 获取。
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone)]
pub struct TradeOrder {
    /// 订单票据号。
    pub ticket: i64,
    /// 下单时间（Unix 秒）。
    pub time_setup: i64,
    /// 下单时间（毫秒精度）。
    pub time_setup_msc: i64,
    /// 订单完成时间（Unix 秒，0 表示尚未完成）。
    pub time_done: i64,
    /// 订单完成时间（毫秒精度）。
    pub time_done_msc: i64,
    /// 订单到期时间（Unix 秒，0 表示永不到期）。
    pub time_expiration: i64,
    /// 订单类型（`ORDER_TYPE_*` 常量：市价单、限价单、止损单等）。
    pub r#type: i32,
    /// 订单有效期类型（`ORDER_TIME_*` 常量：GTC、当日有效、指定时间、到期）。
    pub type_time: i32,
    /// 订单成交类型（`ORDER_FILLING_*` 常量：FOK、IOC、RETURN）。
    pub type_filling: i32,
    /// 订单状态（`ORDER_STATE_*` 常量：已开始、已接受、已取消等）。
    pub state: i32,
    /// 魔法数。
    pub magic: i64,
    /// 关联的持仓标识符。
    pub position_id: i64,
    /// 反向持仓标识符（用于平仓单）。
    pub position_by_id: i64,
    /// 下单原因（`ORDER_REASON_*` 常量）。
    pub reason: i32,
    /// 初始下单量（手）。
    pub volume_initial: f64,
    /// 当前剩余未成交量（手）。
    pub volume_current: f64,
    /// 订单价格。
    pub price_open: f64,
    /// 当前市价。
    pub price_current: f64,
    /// 止损价。
    pub price_sl: f64,
    /// 止盈价。
    pub price_tp: f64,
    /// 止损限价单的触发后限价（StopLimit 订单）。
    pub price_stoplimit: f64,
    /// 交易品种名称。
    pub symbol: String,
    /// 订单备注。
    pub comment: String,
    /// 外部系统标识符。
    pub external_id: String,
}

/// 一笔成交记录（对应 MQL5 的 `DEAL_*` 属性集）。
///
/// 通过 [`crate::Mt5Client::history_deals_get`] 获取。
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone)]
pub struct TradeDeal {
    /// 成交票据号。
    pub ticket: i64,
    /// 产生该成交的订单票据号。
    pub order: i64,
    /// 成交时间（Unix 秒）。
    pub time: i64,
    /// 成交时间（毫秒精度）。
    pub time_msc: i64,
    /// 成交类型（`DEAL_TYPE_*` 常量：买入、卖出、存款、取款等）。
    pub r#type: i32,
    /// 成交方向（`DEAL_ENTRY_*` 常量：开仓、平仓、反向、加减仓）。
    pub entry: i32,
    /// 魔法数。
    pub magic: i64,
    /// 关联的持仓标识符。
    pub position_id: i64,
    /// 成交原因（`DEAL_REASON_*` 常量）。
    pub reason: i32,
    /// 成交量（手）。
    pub volume: f64,
    /// 成交价格。
    pub price: f64,
    /// 佣金。
    pub commission: f64,
    /// 隔夜利息（掉期）。
    pub swap: f64,
    /// 该笔成交的盈亏。
    pub profit: f64,
    /// 手续费。
    pub fee: f64,
    /// 交易品种名称。
    pub symbol: String,
    /// 成交备注。
    pub comment: String,
    /// 外部系统标识符。
    pub external_id: String,
}

/// 市场深度（DOM）中的一档报价（对应 MQL5 的 `BookInfo` 结构体）。
///
/// 通过 [`crate::Mt5Client::market_book_get`] 获取（需先 [`crate::Mt5Client::market_book_add`] 订阅）。
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone)]
pub struct BookInfo {
    /// 档位类型（`BOOK_TYPE_*` 常量：0=卖价，1=买价，2=卖量，3=买量）。
    pub r#type: i64,
    /// 档位价格。
    pub price: f64,
    /// 档位成交量（整数部分，手）。
    pub volume: i64,
    /// 档位实际成交量（双精度，手）。
    pub volume_real: f64,
}

// ============================================================================
// 交易请求与交易结果（order_send / order_check 使用）
// ============================================================================

/// 交易请求（对应 MQL5 的 `MqlTradeRequest` 结构体）。
///
/// 由 [`crate::Mt5Client::order_send`] 或 [`crate::Mt5Client::order_check`] 使用。
///
/// # 构造方式
///
/// 本结构实现了 [`Default`]（所有字段为 0 / 空字符串），
/// 推荐用结构体更新语法只填写需要的字段，与 Python 中省略字典字段的行为一致：
///
/// ```no_run
/// use mt5_rs::{Mt5Client, TradeRequest, TRADE_ACTION_DEAL, ORDER_TYPE_BUY, ORDER_TIME_GTC};
///
/// # fn build(client: &Mt5Client, ask: f64) {
/// let request = TradeRequest {
///     action: TRADE_ACTION_DEAL,   // 市价单
///     symbol: "EURUSD".into(),
///     volume: 0.1,
///     r#type: ORDER_TYPE_BUY,      // 买入
///     price: ask,
///     deviation: 20,               // 最大滑点 20 点
///     type_time: ORDER_TIME_GTC,   // 一直有效
///     comment: "rust open".into(),
///     ..Default::default()
/// };
/// # }
/// ```
///
/// # 注意事项
///
/// 字段的取值约定（`TRADE_ACTION_*`、`ORDER_TYPE_*`、`ORDER_FILLING_*`、
/// `ORDER_TIME_*` 常量）与 Python `MetaTrader5` 库完全一致；具体交易操作需要
/// 哪些字段，可参考 MQL5 文档中 `OrderSend` 的说明。
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, Default)]
pub struct TradeRequest {
    /// 交易请求类型（`TRADE_ACTION_*` 常量，如 `TRADE_ACTION_DEAL`=市价单）。
    pub action: i32,
    /// 魔法数（EA 标识，用于区分自己发出的订单）。
    pub magic: i64,
    /// 订单单号（修改/删除挂单时填写，其余操作为 0）。
    pub order: i64,
    /// 交易品种名称（如 `"EURUSD"`；修改/平仓某些操作可省略）。
    pub symbol: String,
    /// 请求的交易量（手）。
    pub volume: f64,
    /// 执行价格（市价单可填 0 或当前价；挂单必须填触发价）。
    pub price: f64,
    /// 止损限价单的限价（当价格达到 `price` 后以该限价挂单）。
    pub stoplimit: f64,
    /// 止损价（0 表示不设置）。
    pub sl: f64,
    /// 止盈价（0 表示不设置）。
    pub tp: f64,
    /// 请求价格与市价的最大可接受偏差（点，用于滑点控制）。
    pub deviation: i64,
    /// 订单类型（`ORDER_TYPE_*` 常量，如 `ORDER_TYPE_BUY`=市价买入）。
    pub r#type: i32,
    /// 订单成交类型（`ORDER_FILLING_*` 常量：FOK / IOC / RETURN / BOC）。
    pub type_filling: i32,
    /// 订单有效期类型（`ORDER_TIME_*` 常量：GTC / DAY / SPECIFIED / SPECIFIED_DAY）。
    pub type_time: i32,
    /// 挂单到期时间（Unix 秒，仅 `type_time` 为 SPECIFIED / SPECIFIED_DAY 时有效）。
    pub expiration: i64,
    /// 订单注释（最多 31 个字符，超出部分会被截断）。
    pub comment: String,
    /// 持仓单号（修改止损止盈、平仓时填写，用于标识目标持仓）。
    pub position: i64,
    /// 反向持仓单号（`TRADE_ACTION_CLOSE_BY` 反向平仓时填写）。
    pub position_by: i64,
}

/// 交易执行结果（对应 MQL5 的 `MqlTradeResult` 结构体）。
///
/// 由 [`crate::Mt5Client::order_send`] 返回。
///
/// # 判断成功
///
/// `order_send` 不会因业务失败返回 `Err`（与 Python `MetaTrader5` 库一致），
/// 需检查 `retcode`：
///
/// - [`TRADE_RETCODE_DONE`]（10009）= 请求完成（成功）；
/// - [`TRADE_RETCODE_PLACED`]（10008）= 挂单已放置（成功）；
/// - 其余取值见 [`TRADE_RETCODE_REQUOTE`] 起的 `TRADE_RETCODE_*` 常量表。
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone)]
pub struct TradeResult {
    /// 交易服务器返回码（`TRADE_RETCODE_*` 常量，`10009`=成功）。
    pub retcode: i32,
    /// 成交单号（成交时有效，否则为 0）。
    pub deal: i64,
    /// 订单单号（放置挂单时有效，否则为 0）。
    pub order: i64,
    /// 实际成交量（手）。
    pub volume: f64,
    /// 实际成交价。
    pub price: f64,
    /// 当前买价（Bid）。
    pub bid: f64,
    /// 当前卖价（Ask）。
    pub ask: f64,
    /// 服务器返回的注释（如 "Request executed"）。
    pub comment: String,
    /// 请求 ID（终端为本次请求分配的编号）。
    pub request_id: i64,
    /// 外部系统返回码（由外部执行系统返回，0 表示无）。
    pub retcode_external: i32,
}

/// 交易请求检查结果（对应 MQL5 的 `MqlTradeCheckResult` 结构体）。
///
/// 由 [`crate::Mt5Client::order_check`] 返回。
///
/// # 判断成功
///
/// `retcode` 为 `0` 表示检查通过（资金充足、请求有效），
/// 非 0 时 `comment` 包含失败原因。
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone)]
pub struct TradeCheckResult {
    /// 检查返回码（`0` = 检查通过）。
    pub retcode: i32,
    /// 检查后账户余额。
    pub balance: f64,
    /// 检查后账户净值。
    pub equity: f64,
    /// 检查后浮动盈亏。
    pub profit: f64,
    /// 检查后占用的保证金。
    pub margin: f64,
    /// 检查后可用（自由）保证金。
    pub margin_free: f64,
    /// 检查后保证金水平（百分比）。
    pub margin_level: f64,
    /// 服务器返回的注释（如 "Done"）。
    pub comment: String,
}

// ============================================================================
// 交易常量（与 Python `MetaTrader5` 库的同名常量一一对应）
// ============================================================================

// ── 交易请求类型（TradeRequest::action）──

/// 市价单：以当前市场价格立即成交。
pub const TRADE_ACTION_DEAL: i32 = 1;
/// 挂单：在指定条件下成交。
pub const TRADE_ACTION_PENDING: i32 = 5;
/// 修改持仓的止损/止盈。
pub const TRADE_ACTION_SLTP: i32 = 6;
/// 修改已下挂单的参数。
pub const TRADE_ACTION_MODIFY: i32 = 7;
/// 删除已下的挂单。
pub const TRADE_ACTION_REMOVE: i32 = 8;
/// 用反向持仓平仓。
pub const TRADE_ACTION_CLOSE_BY: i32 = 10;

// ── 订单类型（TradeRequest::r#type）──

/// 市价买入。
pub const ORDER_TYPE_BUY: i32 = 0;
/// 市价卖出。
pub const ORDER_TYPE_SELL: i32 = 1;
/// 买入限价单：价格跌至指定价以下时买入。
pub const ORDER_TYPE_BUY_LIMIT: i32 = 2;
/// 卖出限价单：价格涨至指定价以上时卖出。
pub const ORDER_TYPE_SELL_LIMIT: i32 = 3;
/// 买入止损单：价格涨破指定价后买入。
pub const ORDER_TYPE_BUY_STOP: i32 = 4;
/// 卖出止损单：价格跌破指定价后卖出。
pub const ORDER_TYPE_SELL_STOP: i32 = 5;
/// 买入止损限价单：价格达到触发价后挂买入限价单。
pub const ORDER_TYPE_BUY_STOP_LIMIT: i32 = 6;
/// 卖出止损限价单：价格达到触发价后挂卖出限价单。
pub const ORDER_TYPE_SELL_STOP_LIMIT: i32 = 7;
/// 反向平仓。
pub const ORDER_TYPE_CLOSE_BY: i32 = 8;

// ── 订单成交类型（TradeRequest::type_filling）──

/// FOK：全部成交或全部取消。
pub const ORDER_FILLING_FOK: i32 = 0;
/// IOC：按当前市场最大可成交量成交，剩余部分取消。
pub const ORDER_FILLING_IOC: i32 = 1;
/// RETURN：部分成交后剩余部分继续挂单等待。
pub const ORDER_FILLING_RETURN: i32 = 2;
/// BOC：市价单全部成交或全部取消。
pub const ORDER_FILLING_BOC: i32 = 3;

// ── 订单有效期类型（TradeRequest::type_time）──

/// GTC：订单一直有效，直到手动取消。
pub const ORDER_TIME_GTC: i32 = 0;
/// DAY：订单仅在当前交易日有效。
pub const ORDER_TIME_DAY: i32 = 1;
/// SPECIFIED：订单在 `expiration` 指定时间之前有效。
pub const ORDER_TIME_SPECIFIED: i32 = 2;
/// SPECIFIED_DAY：订单在 `expiration` 指定日的 23:59:59 之前有效。
pub const ORDER_TIME_SPECIFIED_DAY: i32 = 3;

// ── 交易服务器返回码（TradeResult::retcode / TradeCheckResult::retcode）──

/// 检查通过（仅用于 `order_check`，线上实测返回 0 表示成功）。
pub const TRADE_RETCODE_OK: i32 = 0;
/// 重新报价（请求被拒绝，需用新价格重试）。
pub const TRADE_RETCODE_REQUOTE: i32 = 10004;
/// 请求被拒绝。
pub const TRADE_RETCODE_REJECT: i32 = 10006;
/// 请求被交易者取消。
pub const TRADE_RETCODE_CANCEL: i32 = 10007;
/// 挂单已放置（成功）。
pub const TRADE_RETCODE_PLACED: i32 = 10008;
/// 请求已完成（成功，订单已成交）。
pub const TRADE_RETCODE_DONE: i32 = 10009;
/// 请求仅部分完成。
pub const TRADE_RETCODE_DONE_PARTIAL: i32 = 10010;
/// 请求处理出错。
pub const TRADE_RETCODE_ERROR: i32 = 10011;
/// 请求超时被取消。
pub const TRADE_RETCODE_TIMEOUT: i32 = 10012;
/// 请求无效。
pub const TRADE_RETCODE_INVALID: i32 = 10013;
/// 请求中的交易量无效。
pub const TRADE_RETCODE_INVALID_VOLUME: i32 = 10014;
/// 请求中的价格无效。
pub const TRADE_RETCODE_INVALID_PRICE: i32 = 10015;
/// 请求中的止损/止盈无效。
pub const TRADE_RETCODE_INVALID_STOPS: i32 = 10016;
/// 交易被禁用。
pub const TRADE_RETCODE_TRADE_DISABLED: i32 = 10017;
/// 市场已关闭。
pub const TRADE_RETCODE_MARKET_CLOSED: i32 = 10018;
/// 资金不足，无法完成请求。
pub const TRADE_RETCODE_NO_MONEY: i32 = 10019;
/// 价格已变化。
pub const TRADE_RETCODE_PRICE_CHANGED: i32 = 10020;
/// 没有可处理请求的报价。
pub const TRADE_RETCODE_PRICE_OFF: i32 = 10021;
/// 请求中的订单到期时间无效。
pub const TRADE_RETCODE_INVALID_EXPIRATION: i32 = 10022;
/// 订单状态已改变。
pub const TRADE_RETCODE_ORDER_CHANGED: i32 = 10023;
/// 请求过于频繁。
pub const TRADE_RETCODE_TOO_MANY_REQUESTS: i32 = 10024;
/// 请求没有变化。
pub const TRADE_RETCODE_NO_CHANGES: i32 = 10025;
/// 服务器禁止自动交易。
pub const TRADE_RETCODE_SERVER_DISABLES_AT: i32 = 10026;
/// 客户端终端禁止自动交易。
pub const TRADE_RETCODE_CLIENT_DISABLES_AT: i32 = 10027;
/// 请求被锁定处理。
pub const TRADE_RETCODE_LOCKED: i32 = 10028;
/// 订单或持仓被冻结。
pub const TRADE_RETCODE_FROZEN: i32 = 10029;
/// 订单成交类型无效。
pub const TRADE_RETCODE_INVALID_FILL: i32 = 10030;
/// 与交易服务器没有连接。
pub const TRADE_RETCODE_CONNECTION: i32 = 10031;
/// 该操作仅允许真实账户。
pub const TRADE_RETCODE_ONLY_REAL: i32 = 10032;
/// 挂单数量已达上限。
pub const TRADE_RETCODE_LIMIT_ORDERS: i32 = 10033;
/// 品种的订单与持仓总量已达上限。
pub const TRADE_RETCODE_LIMIT_VOLUME: i32 = 10034;
/// 订单类型不正确或被禁止。
pub const TRADE_RETCODE_INVALID_ORDER: i32 = 10035;
/// 指定持仓已平仓。
pub const TRADE_RETCODE_POSITION_CLOSED: i32 = 10036;
/// 平仓量超过当前持仓量。
pub const TRADE_RETCODE_INVALID_CLOSE_VOLUME: i32 = 10038;
/// 该持仓已存在平仓订单。
pub const TRADE_RETCODE_CLOSE_ORDER_EXIST: i32 = 10039;
/// 同时持仓数量已达上限。
pub const TRADE_RETCODE_LIMIT_POSITIONS: i32 = 10040;
/// 挂单激活请求被拒绝，订单被取消。
pub const TRADE_RETCODE_REJECT_CANCEL: i32 = 10041;
/// 品种只允许持有多头仓位。
pub const TRADE_RETCODE_LONG_ONLY: i32 = 10042;
/// 品种只允许持有空头仓位。
pub const TRADE_RETCODE_SHORT_ONLY: i32 = 10043;
/// 品种只允许平仓。
pub const TRADE_RETCODE_CLOSE_ONLY: i32 = 10044;
/// 只能按 FIFO 规则平仓（先进先出）。
pub const TRADE_RETCODE_FIFO_CLOSE: i32 = 10045;
/// 禁止对冲持仓。
pub const TRADE_RETCODE_HEDGE_PROHIBITED: i32 = 10046;
