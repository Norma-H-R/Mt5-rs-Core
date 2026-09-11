//! mt5-rs 全接口测试示例（demo）。
//!
//! 调用 `Mt5Client` 的**全部公开接口**（34 个方法 + 管道发现函数），
//! 逐个执行并打印结果摘要，用于验证库与 MT5 终端的连通性与数据正确性。
//!
//! # 运行方式
//!
//! ```text
//! cargo run --example demo              # 安全模式：不真实下单
//! cargo run --example demo -- --send    # 真实下单模式：发送 0.01 手市价买单（有资金风险！）
//! ```
//!
//! # 环境要求
//!
//! - MT5 终端必须正在运行且已登录账户，否则程序会提示未找到管道并退出；
//! - 可选环境变量（用于测试 `login` 接口）：
//!   `MT5_LOGIN`（账号）、`MT5_PASSWORD`（密码）、`MT5_SERVER`（服务器）。

use std::time::{SystemTime, UNIX_EPOCH};

use mt5_rs::{
    discover_mt5_pipe, Mt5Client, TradeRequest, TRADE_ACTION_DEAL, TRADE_ACTION_REMOVE,
    ORDER_FILLING_FOK, ORDER_FILLING_IOC, ORDER_FILLING_RETURN, ORDER_TIME_GTC, ORDER_TYPE_BUY,
};

// MT5 的 PERIOD_* 周期枚举值（注意：不是 MT4 的分钟数！）
const TIMEFRAME_M1: i32 = 1;    // 1 分钟
const TIMEFRAME_H1: i32 = 16385; // 1 小时
const TIMEFRAME_D1: i32 = 16408; // 日线

/// 当前 Unix 时间戳（秒）。
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("系统时间早于 1970")
        .as_secs() as i64
}

/// 调用一个接口并打印结果：成功返回 `Some(值)`，失败打印错误后返回 `None`（继续后续测试）。
fn step<T>(n: usize, name: &str, f: impl FnOnce() -> mt5_rs::Result<T>) -> Option<T> {
    match f() {
        Ok(v) => {
            println!("[{:02}/35] {:<36} => Ok", n, name);
            Some(v)
        }
        Err(e) => {
            println!("[{:02}/35] {:<36} => Err: {}", n, name, e);
            None
        }
    }
}

fn main() {
    // 是否真实下单（默认关闭；开启后会用账户真实资金发送 0.01 手市价买单）
    let really_send = std::env::args().any(|a| a == "--send");
    println!("================================================");
    println!(" mt5-rs 全接口测试 demo（真实下单模式: {}）", really_send);
    println!("================================================");

    // [01] discover_mt5_pipe：自动发现 MT5 终端的命名管道。
    // 无终端时该函数会 panic，这里临时静默 panic 输出并捕获，转为友好提示
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let discovered = std::panic::catch_unwind(discover_mt5_pipe);
    std::panic::set_hook(default_hook);

    println!("[01/35] discover_mt5_pipe                          => ...");
    let pipe_name = match discovered {
        Ok(name) => {
            println!("[01/35] discover_mt5_pipe                          => Ok: {}", name);
            name
        }
        Err(_) => {
            println!("[01/35] discover_mt5_pipe                          => Err: 未找到 MT5 管道");
            println!();
            println!("测试中止：请先启动 MetaTrader 5 终端并登录账户，然后重新运行本示例。");
            std::process::exit(1);
        }
    };

    // [02] Mt5Client::new + initialize：建立连接（握手获取 build 号）
    let mut client = Mt5Client::new();
    if step(2, "Mt5Client::new + initialize", || {
        client.initialize(Some(&pipe_name))
    })
    .is_none()
    {
        println!("测试中止：连接 MT5 失败。");
        std::process::exit(1);
    }

    // ---------- 账户与终端 ----------
    println!("\n--- 账户与终端 ---");

    // [03] account_info：账户信息
    let account = step(3, "account_info", || client.account_info());
    if let Some(a) = account {
        println!("          login={}  server={}  currency={}", a.login, a.server, a.currency);
        println!("          balance={}  equity={}  margin={}  free_margin={}  margin_level={}",
            a.balance, a.equity, a.margin, a.free_margin, a.margin_level);
    }

    // [04] terminal_info：终端信息
    let terminal = step(4, "terminal_info", || client.terminal_info());
    if let Some(t) = terminal {
        println!("          build={}  connected={}  trade_allowed={}  company={}  path={}",
            t.build, t.connected, t.trade_allowed, t.company, t.path);
    }

    // [05] version：版本信息
    let version = step(5, "version", || client.version());
    if let Some(v) = version {
        println!("          version={}  build={}  date={}", v.version, v.build, v.build_date);
    }

    // [06] login：登录（凭据来自环境变量，未设置则跳过调用）
    let mt5_login = std::env::var("MT5_LOGIN").ok();
    let mt5_password = std::env::var("MT5_PASSWORD").ok();
    let mt5_server = std::env::var("MT5_SERVER").ok();
    match (mt5_login, mt5_password, mt5_server) {
        (Some(l), Some(p), Some(s)) => {
            let login: i64 = match l.parse() {
                Ok(v) => v,
                Err(_) => {
                    println!("[06/35] login                                    => 跳过（MT5_LOGIN 不是数字）");
                    0
                }
            };
            let _ = step(6, "login", || client.login(login, &p, &s));
        }
        _ => println!("[06/35] login                                    => 跳过（未设置 MT5_LOGIN/MT5_PASSWORD/MT5_SERVER）"),
    }

    // ---------- 交易品种 ----------
    println!("\n--- 交易品种 ---");

    // [07] symbols_total：品种总数
    let total = step(7, "symbols_total", || client.symbols_total());
    if let Some(t) = total {
        println!("          品种总数 = {}", t);
    }

    // [08] symbols_get：全部品种信息（打印前 3 个）
    let symbols = step(8, "symbols_get", || client.symbols_get());
    let demo_symbol = symbols.as_ref().and_then(|v| v.first().map(|s| s.name.clone()));
    if let Some(list) = symbols.as_ref() {
        println!("          共 {} 个品种，前 3 个:", list.len());
        for s in list.iter().take(3) {
            println!("            {}  bid={}  ask={}  point={}", s.name, s.bid, s.ask, s.point);
        }
    }

    // 默认测试品种：优先 EURUSD（流动性好、深度数据全），否则取第一个品种
    let symbol = symbols
        .as_ref()
        .and_then(|v| v.iter().find(|s| s.name == "EURUSD"))
        .map(|s| s.name.clone())
        .or(demo_symbol)
        .unwrap_or_else(|| "EURUSD".to_string());
    println!("          测试品种选定: {}", symbol);

    // [09] symbol_info：单个品种信息
    let info = step(9, "symbol_info", || client.symbol_info(&symbol));
    if let Some(Some(i)) = info {
        println!("          {}  digits={}  contract_size={}  volume_min={}  volume_max={}",
            i.name, i.digits, i.trade_contract_size, i.volume_min, i.volume_max);
    }

    // [10] symbol_info_tick：最新报价
    let tick = step(10, "symbol_info_tick", || client.symbol_info_tick(&symbol));
    if let Some(Some(t)) = tick.as_ref() {
        println!("          time={}  bid={}  ask={}  last={}  volume_real={}", t.time, t.bid, t.ask, t.last, t.volume_real);
    }

    // [11] symbol_select：在市场报价中选中品种
    let _ = step(11, "symbol_select(选中)", || client.symbol_select(&symbol, true));
    let _ = step(11, "symbol_select(取消)", || client.symbol_select(&symbol, false));
    let _ = step(11, "symbol_select(重新选中)", || client.symbol_select(&symbol, true));

    // ---------- 行情数据 ----------
    println!("\n--- 行情数据 ---");

    // [12] copy_rates_from_pos：从当前位置复制 K 线
    let rates = step(12, "copy_rates_from_pos", || {
        client.copy_rates_from_pos(&symbol, TIMEFRAME_M1, 0, 10)
    });
    if let Some(r) = rates {
        println!("          共 {} 根 M1 K线，最新: time={} O={} H={} L={} C={}",
            r.len(),
            r.last().map(|x| x.time).unwrap_or(0),
            r.last().map(|x| x.open).unwrap_or(0.0),
            r.last().map(|x| x.high).unwrap_or(0.0),
            r.last().map(|x| x.low).unwrap_or(0.0),
            r.last().map(|x| x.close).unwrap_or(0.0));
    }

    // [13] copy_rates_from：从指定时间开始复制 K 线
    let rates = step(13, "copy_rates_from", || {
        client.copy_rates_from(&symbol, TIMEFRAME_D1, now() - 86400 * 30, 30) // 近 30 天
    });
    if let Some(r) = rates {
        println!("          共 {} 根 D1 K线", r.len());
    }

    // [14] copy_rates_range：复制时间范围内的 K 线
    // 注意：to 需留出时钟余量（本地时钟通常比服务器快数秒~数分钟），
    // 若 to 落在服务器"未来"，终端会返回空数据
    let rates = step(14, "copy_rates_range", || {
        client.copy_rates_range(&symbol, TIMEFRAME_H1, now() - 172800, now() - 600) // 近 48 小时
    });
    if let Some(r) = rates {
        println!("          共 {} 根 H1 K线", r.len());
    }

    // [15] copy_ticks_from：从指定时间开始复制 Tick
    let ticks = step(15, "copy_ticks_from", || {
        client.copy_ticks_from(&symbol, now() - 86400, 100, -1) // 近 1 天，最多 100 条，全部类型
    });
    if let Some(t) = ticks.as_ref() {
        println!("          共 {} 条 Tick，首条 time={}", t.len(), t.first().map(|x| x.time).unwrap_or(0));
    }

    // [16] copy_ticks_range：复制时间范围内的 Tick
    // 注意：to 需留出时钟余量（本地时钟通常比服务器快数秒~数分钟），
    // 若 to 落在服务器"未来"，终端会返回空数据
    let ticks = step(16, "copy_ticks_range", || {
        client.copy_ticks_range(&symbol, now() - 86400 * 2, now() - 600, -1) // 近 2 天
    });
    if let Some(t) = ticks {
        println!("          共 {} 条 Tick", t.len());
    }

    // ---------- 持仓与订单 ----------
    println!("\n--- 持仓与订单 ---");

    // [17] positions_total：持仓总数
    let total = step(17, "positions_total", || client.positions_total());
    if let Some(t) = total {
        println!("          持仓总数 = {}", t);
    }

    // [18] positions_get：获取全部持仓（再按品种过滤一次）
    let positions = step(18, "positions_get(全部)", || client.positions_get(None));
    if let Some(p) = positions {
        println!("          共 {} 笔持仓", p.len());
        for pos in p.iter().take(3) {
            println!("            ticket={}  {}  {}  {}手  open={}  profit={}",
                pos.ticket, pos.symbol, if pos.r#type == 0 { "BUY" } else { "SELL" },
                pos.volume, pos.price_open, pos.profit);
        }
    }
    let _ = step(18, "positions_get(按品种)", || client.positions_get(Some(&symbol)));

    // [19] orders_total：挂单总数
    let total = step(19, "orders_total", || client.orders_total());
    if let Some(t) = total {
        println!("          挂单总数 = {}", t);
    }

    // [20] orders_get：获取全部挂单
    let orders = step(20, "orders_get(全部)", || client.orders_get(None));
    if let Some(o) = orders {
        println!("          共 {} 笔挂单", o.len());
        for od in o.iter().take(3) {
            println!("            ticket={}  {}  {}  {}手  price={}",
                od.ticket, od.symbol, od.r#type, od.volume_initial, od.price_open);
        }
    }
    let _ = step(20, "orders_get(按品种)", || client.orders_get(Some(&symbol)));

    // ---------- 历史数据 ----------
    println!("\n--- 历史数据 ---");

    // [21] history_deals_total / [22] history_deals_get：近 7 天成交
    let deals_total = step(21, "history_deals_total", || {
        client.history_deals_total(now() - 86400 * 7, now())
    });
    if let Some(t) = deals_total {
        println!("          近 7 天成交总数 = {}", t);
    }
    let deals = step(22, "history_deals_get", || {
        client.history_deals_get(now() - 86400 * 7, now())
    });
    if let Some(d) = deals {
        println!("          共 {} 笔成交", d.len());
        for dl in d.iter().take(3) {
            let sym = if dl.symbol.is_empty() { "(账户操作)".to_string() } else { dl.symbol.clone() };
            println!("            ticket={}  {}  type={}  {}手  price={}  profit={}",
                dl.ticket, sym, dl.r#type, dl.volume, dl.price, dl.profit);
        }
    }

    // [23] history_orders_total / [24] history_orders_get：近 7 天订单历史
    let orders_total = step(23, "history_orders_total", || {
        client.history_orders_total(now() - 86400 * 7, now())
    });
    if let Some(t) = orders_total {
        println!("          近 7 天订单总数 = {}", t);
    }
    let orders = step(24, "history_orders_get", || {
        client.history_orders_get(now() - 86400 * 7, now())
    });
    if let Some(o) = orders {
        println!("          共 {} 笔历史订单", o.len());
    }

    // ---------- 市场深度 ----------
    println!("\n--- 市场深度 ---");

    // [25] market_book_add：订阅市场深度
    let _ = step(25, "market_book_add", || client.market_book_add(&symbol));

    // [26] market_book_get：获取市场深度
    let book = step(26, "market_book_get", || client.market_book_get(&symbol));
    if let Some(b) = book {
        println!("          共 {} 档", b.len());
        for e in b.iter().take(5) {
            println!("            type={}  price={}  volume={}", e.r#type, e.price, e.volume);
        }
    }

    // [27] market_book_release：取消订阅
    let _ = step(27, "market_book_release", || client.market_book_release(&symbol));

    // ---------- 交易计算 ----------
    println!("\n--- 交易计算 ---");

    // [28] order_calc_margin：计算保证金（本地计算）
    let ask = tick.as_ref().and_then(|t| t.as_ref()).map(|t| t.ask).unwrap_or(1.0);
    // 打印品种的初始保证金比例，便于核对本地计算公式的输入
    if let Some(Some(i)) = client.symbol_info(&symbol).ok() {
        println!("          (品种信息: margin_initial={} trade_contract_size={})",
            i.margin_initial, i.trade_contract_size);
    }
    let margin = step(28, "order_calc_margin", || {
        client.order_calc_margin(TRADE_ACTION_DEAL, &symbol, 0.1, ask)
    });
    if let Some(m) = margin {
        println!("          0.1 手买入所需保证金 = {}", m);
    }

    // [29] order_calc_profit：计算预期利润（本地计算）
    let profit = step(29, "order_calc_profit", || {
        client.order_calc_profit(TRADE_ACTION_DEAL, &symbol, 0.1, ask, ask + 0.01)
    });
    if let Some(p) = profit {
        println!("          0.1 手买入、涨 0.01 后的利润 = {}", p);
    }

    // ---------- 交易请求 ----------
    println!("\n--- 交易请求 ---");

    // [30] order_check：检查交易请求（只检查，不成交，安全）。
    // 不同品种支持的成交模式不同，依次尝试 FOK / IOC / RETURN，直到检查通过
    println!("[30/35] order_check(0.01手买入)                        => ...");
    let mut check_passed = false;
    for (filling, filling_name) in [
        (ORDER_FILLING_FOK, "FOK"),
        (ORDER_FILLING_IOC, "IOC"),
        (ORDER_FILLING_RETURN, "RETURN"),
    ] {
        let check_request = TradeRequest {
            action: TRADE_ACTION_DEAL,
            symbol: symbol.clone(),
            volume: 0.01,
            r#type: ORDER_TYPE_BUY,
            price: ask,
            deviation: 20,
            type_filling: filling,
            type_time: ORDER_TIME_GTC,
            comment: "mt5-rs demo check".into(),
            ..Default::default()
        };
        match client.order_check(&check_request) {
            Ok(c) => {
                println!("          [{}] retcode={}  balance={}  equity={}  margin={}  margin_free={}  margin_level={}  comment={}",
                    filling_name, c.retcode, c.balance, c.equity, c.margin, c.margin_free, c.margin_level, c.comment);
                if c.retcode == 0 {
                    check_passed = true;
                    break;
                }
            }
            Err(e) => println!("          [{}] Err: {}", filling_name, e),
        }
    }
    if check_passed {
        println!("[30/35] order_check                                   => Ok（检查通过）");
    } else {
        println!("[30/35] order_check                                   => 未通过（retcode 见上，属业务拒绝，接口本身工作正常）");
    }

    // [31] order_send：发送交易请求
    if really_send {
        // 真实下单模式：发送 0.01 手市价买单（会真实成交！）
        println!("          ⚠ 真实下单模式：即将发送 0.01 手市价买单");
        let send_request = TradeRequest {
            action: TRADE_ACTION_DEAL,
            symbol: symbol.clone(),
            volume: 0.01,
            r#type: ORDER_TYPE_BUY,
            price: ask,
            deviation: 20,
            type_filling: ORDER_FILLING_RETURN,
            type_time: ORDER_TIME_GTC,
            comment: "mt5-rs demo send".into(),
            ..Default::default()
        };
        let result = step(31, "order_send(0.01手市价买单)", || client.order_send(&send_request));
        if let Some(r) = result {
            println!("          retcode={}  deal={}  order={}  volume={}  price={}  comment={}",
                r.retcode, r.deal, r.order, r.volume, r.price, r.comment);
        }
    } else {
        // 安全模式：发送一个必然失败的请求（删除不存在的订单），验证链路但绝不产生交易
        let safe_request = TradeRequest {
            action: TRADE_ACTION_REMOVE, // 删除挂单
            order: 0,                    // 不存在的订单号 → 服务器必然返回错误
            ..Default::default()
        };
        let result = step(31, "order_send(删除不存在的订单, 链路测试)", || client.order_send(&safe_request));
        if let Some(r) = result {
            println!("          retcode={}  comment={}", r.retcode, r.comment);
            println!("          （retcode 非 10009/10008 属预期：服务器拒绝删除不存在的订单，链路已走通）");
        }
    }

    // ---------- 高级接口与错误 ----------
    println!("\n--- 高级接口与错误 ---");

    // [32] send_raw_command：发送原始命令（190 = 账户信息）
    let raw = step(32, "send_raw_command(190)", || client.send_raw_command(190, &[]));
    if let Some(b) = raw {
        println!("          返回 {} 字节原始数据", b.len());
    }

    // [33] last_error：最近一次错误（正常为 0）
    let err = step(33, "last_error", || client.last_error());
    if let Some((code, msg)) = err {
        println!("          code={}  message={}", code, msg);
    }

    // [34] shutdown：关闭连接
    let _ = step(34, "shutdown", || {
        client.shutdown();
        Ok(())
    });

    println!("\n================================================");
    println!(" 测试完成（真实下单模式: {}）", really_send);
    println!(" 提示：上面出现 Err 的接口，请结合 MT5 终端状态排查；");
    println!(" 提示：本机未运行 MT5 时，除初始化外全部接口都会返回 NotInitialized。");
    println!("================================================");
}
