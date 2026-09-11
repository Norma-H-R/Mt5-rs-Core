# mt5-rs（Mt5-rs-Core 分支）

**纯 Rust 实现的 MetaTrader 5 IPC 通信库** —— 无需 Python、无需 C++，通过 Windows 命名管道直连运行中的 `terminal64.exe`。

与 Python 官方 `MetaTrader5` 库 API 完全兼容（函数命名、参数、返回结构一致，32/32 个函数全部实现），
Python 量化交易者可平滑迁移到 Rust。

- **代码仓库**：<https://github.com/Norma-H-R/Mt5-rs-Core>
- **上游来源**：<https://github.com/yjkt/mt5-rs>（MIT，作者 yjkt）
- **许可**：MIT（保留上游版权声明，见 [§12](#12-许可证与出处)）

> **本分支的定位**：上游库解决「能不能通」；本分支在此之上解决「长期跑得稳、下得出手」。
> 相对上游，我们补齐了 **管道读取模式、并发保护、超时保护、多终端发现、异步下单**
> 五项能力，并修正了若干实盘踩过的坑（详见 [§1](#1-相对上游的增强)）。

---

## 目录

1. [相对上游的增强](#1-相对上游的增强)
2. [特性](#2-特性)
3. [快速开始](#3-快速开始)
4. [健壮性能力详解](#4-健壮性能力详解)
5. [下单专题](#5-下单专题)
6. [API 参考](#6-api-参考)
7. [示例](#7-示例)
8. [通信协议说明](#8-通信协议说明)
9. [实现说明](#9-实现说明)
10. [错误处理](#10-错误处理)
11. [系统要求](#11-系统要求)
12. [许可证与出处](#12-许可证与出处)

---

## 1. 相对上游的增强

| # | 能力 | 上游 `yjkt/mt5-rs` | 本分支 |
|---|---|---|---|
| 1 | **管道读取模式** | 未设置 `PIPE_READMODE_MESSAGE` | 连接即设消息模式，避免 `WriteFile` 永久阻塞 |
| 2 | **并发保护** | 无锁；文档要求调用方自行 `Mutex<Mt5Client>` | 内部写锁串行化请求；类型实现 `Send + Sync`，可直接 `Arc` 共享 |
| 3 | **超时保护** | `ReadFile` 无超时，终端假死则线程永久卡住 | 单次请求默认 3s 超时（可调）；超时即标记连接失效，重新 `initialize` 恢复 |
| 4 | **多终端发现** | 仅 `discover_mt5_pipe()`（单终端，找不到 `panic`） | 新增 `discover_all_terminals()`（含 exe 路径）/ `discover_all_mt5_pipes()`；`find_terminal64_paths()` 公开 |
| 5 | **异步下单** | 仅同步 `order_send` | 新增 `order_send_async()`（发出即返回、结果回调），适配跟单等毫秒级场景 |

此外还做了这些工程处理：

- 修正 `copy_ticks_*` 的时间戳单位（上游/`go-mt5` 直接发秒值，终端按毫秒解释 → 返回错误数据）；
- `order_calc_*` 改为本地计算（规避 MT5 Build 5836+ 的 202/203 命令「管道已关闭」问题）；
- `examples/gui.rs` 的 `eframe` 依赖改为**可选 feature**，构建库本体不再拉 GUI 依赖树；
- 补齐 `examples/` 三个示例（基础 API / 健壮性能力 / GUI 调试）。

---

## 2. 特性

- **纯 Rust 实现**：仅依赖 `windows-sys`、`thiserror`、`sha2`、`hex`，无 Python / C++ 依赖；
- **命名管道 IPC**：直连 MT5 终端，支持管道名自动发现与多终端枚举；
- **API 兼容**：与 Python `MetaTrader5` 库 32/32 函数对齐，对照其官方文档即可使用；
- **版本支持**：MT5 Build 5836+（已规避该版本之后部分 IPC 命令的「管道已关闭」问题）；
- **健壮**：消息模式管道 + 并发写锁 + 请求超时 + 失效即拒绝，适配长时间运行的自动化场景；
- **可下单**：`order_check` / `order_send` / `order_send_async`，覆盖市价、挂单、改单、平仓、部分平仓。

### 可选 feature（默认全部关闭）

| feature | 作用 | 开启方式 |
|---|---|---|
| `serde` | 为 `AccountInfo` / `SymbolInfo` / `TradePosition` 等数据结构实现 `serde::Serialize` | `mt5-rs = { path = "../mt5-rs-core", features = ["serde"] }` |
| `gui` | 仅供 `examples/gui.rs`（API 调试 GUI，依赖 `eframe`） | `cargo run --example gui --features gui` |

> 默认关闭是刻意的：纯库使用者不会为此拉入 `serde` / `eframe` 依赖树。

---

## 3. 快速开始

```toml
# Cargo.toml
[dependencies]
mt5-rs = { path = "../mt5-rs-core" }   # 或 git 依赖到 Mt5-rs-Core
```

最小示例（连接 → 读账户 → 关闭）：

```rust
use mt5_rs::{discover_mt5_pipe, Mt5Client};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 自动发现正在运行的 MT5 终端对应的命名管道
    let pipe_name = discover_mt5_pipe();

    // 2. 建立连接（内部：消息模式 + 写锁 + 3 秒超时）
    let mut client = Mt5Client::new();
    client.initialize(Some(&pipe_name))?;

    // 3. 读取账户信息
    let acc = client.account_info()?;
    println!("账号={} 余额={} 净值={} 可用保证金={}",
             acc.login, acc.balance, acc.equity, acc.margin_free);

    // 4. 关闭连接（直接 drop 也会自动释放管道句柄）
    client.shutdown();
    Ok(())
}
```

> **前提**：MT5 终端必须正在运行并已登录账户。多终端场景请改用
> [`discover_all_terminals()`](#4-健壮性能力详解)（`discover_mt5_pipe()` 只返回第一个，
> 且找不到时会 panic）。

---

## 4. 健壮性能力详解

这四项能力在库内部生效，调用方无需额外代码。可运行演示：

```bash
cargo run --example pipe_features
```

### 4.1 管道读取模式（`PIPE_READMODE_MESSAGE`）

**为什么需要**：MT5 终端管道是**消息模式**管道。客户端若以默认的*字节模式*去写消息模式管道，
`WriteFile` 会**永久阻塞**（不是报错、不是超时——是挂死）。Python `MetaTrader5` 与
`go-mt5` 都在连接后立刻设置读取模式。

**怎么用**：无需操作。`NamedPipeClient::new()` / `with_timeout()` 在打开管道后立即设置：

```rust
// src/protocol.rs（库内部节选）
unsafe {
    let mode: u32 = PIPE_READMODE_MESSAGE;
    SetNamedPipeHandleState(handle, &mode, std::ptr::null(), std::ptr::null());
}
```

**注意**：该状态是**句柄级**的，每次打开管道都要设置；若你绕过本库自行
`CreateFileW` 连接 MT5 管道，务必补上这一步，否则第一次调用就会卡死。

### 4.2 并发保护（写锁 + `Send + Sync`）

**为什么需要**：同一条管道句柄上**不允许**并发 `ReadFile` / `WriteFile`：
轻则请求与响应**错配**（把 A 的响应当成 B 的结果，即"串包"），重则永久阻塞。
在跟单/交易场景里，串包是灾难级问题。

**怎么用**：`Mt5Client` 实现了 `Send + Sync`，内部写锁把「写 + 读」整段串行化，
可直接放进 `Arc` 给多线程共享：

```rust
use mt5_rs::Mt5Client;
use std::sync::Arc;

let mut client = Mt5Client::new();
client.initialize(Some(&pipe_name))?;   // 连接需要 &mut self
let client = Arc::new(client);          // 之后只读共享

let handles: Vec<_> = (0..4).map(|_| {
    let c = Arc::clone(&client);
    std::thread::spawn(move || {
        // 多线程并发调用：请求被内部写锁串行化，响应不会串包
        c.account_info().map(|a| a.balance).unwrap_or(0.0)
    })
}).collect();

for h in handles { let _ = h.join(); }
```

**注意**：

- 并发是**安全**而非**并行**——请求仍被串行化（这是管道协议本身的限制）。
  要吞吐就把不同终端拆到不同 `Mt5Client`（每个终端一条管道）；
- `initialize` / `shutdown` 需要 `&mut self`，**必须在放进 `Arc` 之前完成连接**；
- 本库刻意不做「每请求新建连接」：`CreateFileW` 打开的是独占句柄，反复开关既慢，
  也可能撞上终端的连接上限。

### 4.3 超时保护（超时即标记连接失效）

**为什么需要**：Windows 的同步 `ReadFile` **没有超时参数**。终端假死、正在重启、
管道半连接时，调用线程会被永久卡住——在跟单里表现为「引擎线程全堵，连开仓都不响应」。

**怎么用**：默认超时 `DEFAULT_TIMEOUT_MS = 3000ms`，两种调整方式：

```rust
use mt5_rs::{Mt5Client, Mt5Error, DEFAULT_TIMEOUT_MS};

// 方式一：初始化时指定
let mut client = Mt5Client::new();
client.initialize_with_timeout(Some(&pipe_name), 1000)?;

// 方式二：连接后动态调整（例如快照收紧、下单放宽）
client.set_timeout_ms(5000)?;
println!("当前超时 = {:?} ms", client.timeout_ms());
```

超时后的行为与恢复路径：

```rust
match client.account_info() {
    Err(Mt5Error::Timeout(msg)) => {
        // 1) 连接已被标记失效：管道里可能残留响应，继续复用会串包
        assert!(client.is_connection_broken());
        println!("超时：{msg}");
        println!("错误码 = {}", Mt5Error::Timeout(String::new()).error_code()); // -10086

        // 2) 失效后再次调用会立即失败（不会卡住）
        let _ = client.account_info();   // Err(ConnectionFailed: 连接已失效…)

        // 3) 恢复：重新 initialize（内部重开管道并握手）
        client.initialize(Some(&pipe_name))?;
        client.set_timeout_ms(DEFAULT_TIMEOUT_MS)?;
    }
    Err(e) => println!("其它错误：{e}"),
    Ok(acc) => println!("正常：{}", acc.balance),
}
```

**设计取舍（重要）**：超时后本库**不做**「取消 IO + 排空管道」的原地恢复
（那需要 `CancelSynchronousIo` + `OpenThread`，行为依赖系统与终端状态），
而是**直接把连接标记失效**，要求重新 `initialize`。理由：超时属于异常路径，
此时管道状态不可信，「重新建连」比「原地续用」确定性高得多；附带好处是
后台那个阻塞线程在对端进程退出时会因 `ReadFile` 报错而自行结束，不会永久泄漏。

### 4.4 多终端发现

**为什么需要**：一台机器常同时开多个 MT5（每个账户一个实例）。单终端入口只返回第一个，
多账户场景需要「列出全部 + 拿到终端安装路径」。

```rust
use mt5_rs::{compute_pipe_name, discover_all_mt5_pipes, discover_all_terminals};

// (1) exe 路径 + 管道名：既能连，又能定位终端目录（部署 EA/DLL、查日志）
for (exe_path, pipe_name) in discover_all_terminals() {
    println!("终端: {exe_path}\n  管道: {pipe_name}");
}

// (2) 只要管道名
let pipes = discover_all_mt5_pipes();

// (3) 已知终端安装路径 → 自行算管道名（不枚举进程，最快）
let pipe = compute_pipe_name(r"D:\Program Files\MT5\terminal64.exe");
```

多终端逐端连接：

```rust
use mt5_rs::{discover_all_terminals, Mt5Client};

let mut clients = Vec::new();
for (exe_path, pipe_name) in discover_all_terminals() {
    let mut c = Mt5Client::new();
    match c.initialize(Some(&pipe_name)) {
        Ok(()) => {
            let acc = c.account_info()?;
            println!("{exe_path} 已连接：login={} balance={:.2}", acc.login, acc.balance);
            clients.push(c);
        }
        Err(e) => println!("{exe_path} 连接失败：{e}"),
    }
}
```

**行为与注意**：

- `discover_all_terminals()` 只返回**当前可连接**的终端，找不到时返回**空列表**（不 panic）；
- 返回的 exe 路径已去重；`find_terminal64_paths()` 单独获取「所有终端进程路径」（即使管道不可连）；
- `discover_mt5_pipe()` 是**单终端便捷入口**，找不到可连接管道时**会 panic**——
  只适合「确定只有一个终端」的场景。

---

## 5. 下单专题

三种接口的分工：

| 接口 | 语义 | 适用 |
|---|---|---|
| `order_check(&req)` | 预检：请求是否合法、资金是否充足（`retcode == 0` 通过） | 下单前校验，避免无效请求 |
| `order_send(&req)` | 同步下单：等终端返回结果 | 需要立即知道成交结果 |
| `order_send_async(&req, cb)` | 异步下单：发出即返回，结果回调 | 跟单等毫秒级场景，不等回执 |

### 5.1 市价买入（同步）

```rust
use mt5_rs::{
    discover_mt5_pipe, Mt5Client, TradeRequest, ORDER_FILLING_IOC, ORDER_TIME_GTC,
    ORDER_TYPE_BUY, TRADE_ACTION_DEAL, TRADE_RETCODE_DONE, TRADE_RETCODE_PLACED,
};

let mut client = Mt5Client::new();
client.initialize(Some(&discover_mt5_pipe()))?;

let tick = client.symbol_info_tick("EURUSD")?.unwrap();

let req = TradeRequest {
    action: TRADE_ACTION_DEAL,       // 市价单
    symbol: "EURUSD".into(),
    volume: 0.10,
    r#type: ORDER_TYPE_BUY,
    price: tick.ask,
    deviation: 20,                   // 允许 20 点滑点
    type_time: ORDER_TIME_GTC,
    type_filling: ORDER_FILLING_IOC, // ⚠️ 见 5.4
    comment: "mt5-rs order".into(),
    ..Default::default()
};

// 先预检（可选）
let check = client.order_check(&req)?;
if check.retcode != 0 {
    println!("预检未通过：{}", check.comment);
}

// 再下单
let res = client.order_send(&req)?;
if res.retcode == TRADE_RETCODE_DONE || res.retcode == TRADE_RETCODE_PLACED {
    println!("成功：deal={} order={} price={}", res.deal, res.order, res.price);
} else {
    println!("失败：retcode={} comment={}", res.retcode, res.comment);
}
```

> **判断成功**：`TRADE_RETCODE_DONE(10009)`（已成交）或 `TRADE_RETCODE_PLACED(10008)`（已受理）。
> **业务失败不返回 `Err`**：`order_send` 只在协议/IO 层失败时抛错，
> 交易被拒（资金不足、成交模式不支持等）通过 `res.retcode` 表达——**必须自己判断**。

### 5.2 平仓 / 部分平仓

```rust
use mt5_rs::{TradeRequest, ORDER_TYPE_SELL, TRADE_ACTION_DEAL};

// 平掉指定持仓（对冲账户：下反向单 + 指定 position）
let pos = client.positions_get(Some("EURUSD"))?.into_iter().next().unwrap();
let tick = client.symbol_info_tick(&pos.symbol)?.unwrap();

let close_req = TradeRequest {
    action: TRADE_ACTION_DEAL,
    position: pos.ticket,                                  // ← 指明要平的持仓
    symbol: pos.symbol.clone(),
    volume: pos.volume,                                    // 部分平仓就填更小的量
    r#type: if pos.r#type == 0 { ORDER_TYPE_SELL } else { ORDER_TYPE_BUY },
    price: if pos.r#type == 0 { tick.bid } else { tick.ask },
    deviation: 20,
    type_filling: ORDER_FILLING_IOC,
    ..Default::default()
};
let res = client.order_send(&close_req)?;
println!("平仓 retcode={}", res.retcode);
```

### 5.3 改单（止盈止损）与挂单

```rust
use mt5_rs::{TRADE_ACTION_SLTP, TRADE_ACTION_PENDING, TRADE_ACTION_REMOVE,
             ORDER_TYPE_BUY_LIMIT, ORDER_TIME_SPECIFIED};

// 改止盈止损：只需 position + sl/tp
let sltp = TradeRequest {
    action: TRADE_ACTION_SLTP,
    position: pos.ticket,
    symbol: pos.symbol.clone(),
    sl: 1.07000,
    tp: 1.10000,
    ..Default::default()
};
client.order_send(&sltp)?;

// 挂单：限价买
let pending = TradeRequest {
    action: TRADE_ACTION_PENDING,
    symbol: "EURUSD".into(),
    volume: 0.10,
    r#type: ORDER_TYPE_BUY_LIMIT,
    price: 1.08000,
    type_time: ORDER_TIME_SPECIFIED,
    expiration: now_unix + 3600,        // Unix 秒
    ..Default::default()
};
client.order_send(&pending)?;

// 删挂单：action=REMOVE + order=ticket
let remove = TradeRequest { action: TRADE_ACTION_REMOVE, order: order_ticket, ..Default::default() };
client.order_send(&remove)?;
```

### 5.4 成交模式（`type_filling`）—— 最容易踩的坑

**现象**：下单被拒，`retcode = 10030`（`Unsupported filling mode`）。

**原因**：不同品种/经纪商支持的成交模式不同，写死一种必然在部分品种上失败：

| 常量 | 值 | 含义 | 常见于 |
|---|---|---|---|
| `ORDER_FILLING_FOK` | 0 | 全部成交或全部取消 | 多数外汇品种 |
| `ORDER_FILLING_IOC` | 1 | 立即成交可成交量，其余取消 | 多数外汇、黄金 CFD |
| `ORDER_FILLING_RETURN` | 2 | 未成交部分挂回（市场单常用） | 加密/交易所类品种（如 BTCUST） |

**正确做法**：按品种的 `SYMBOL_FILLING_MODE` 位掩码自适应，失败则换下一个候选：

```rust
/// 按品种支持的成交模式给出候选序列（FOK=1 / IOC=2；两位都没有 ⇒ 只吃 RETURN）
fn filling_candidates(client: &Mt5Client, symbol: &str) -> Vec<i32> {
    let mask = client.symbol_info(symbol)?.map(|s| s.filling_mode).unwrap_or(0);
    let mut out = Vec::new();
    if mask & 1 != 0 { out.push(ORDER_FILLING_FOK); }
    if mask & 2 != 0 { out.push(ORDER_FILLING_IOC); }
    if mask & 3 == 0 { out.push(ORDER_FILLING_RETURN); }
    for f in [ORDER_FILLING_IOC, ORDER_FILLING_FOK, ORDER_FILLING_RETURN] {
        if !out.contains(&f) { out.push(f); }
    }
    out
}

// 逐个尝试；只有 10030 才值得换模式（资金不足/休市换模式同样失败）
for f in filling_candidates(&client, &req.symbol) {
    req.type_filling = f;
    let res = client.order_send(&req)?;
    if res.retcode == TRADE_RETCODE_DONE || res.retcode == TRADE_RETCODE_PLACED {
        break;
    }
    if res.retcode != 10030 { break; }   // 不是成交模式问题 → 换模式无意义
}
```

> **实战经验**：同一个经纪商的不同品种可能各支持不同模式；
> **品种属性界面显示的「成交模式」与服务器实际接受的模式不一定一致**
> （遇到过规格页写 IOC、实际只收 FOK 的经纪商）。稳妥策略：
> **先按掩码自适应，再逐个试，全部失败才判定为真实错误**。

### 5.5 异步下单（`order_send_async`）

**用途**：跟单等「发出即算数」的场景——调用方不等待服务器回执，结果在回调里处理。

```rust
use mt5_rs::{Mt5Client, TradeRequest, TRADE_ACTION_DEAL, ORDER_TYPE_BUY};
use std::sync::Arc;

let mut client = Mt5Client::new();
client.initialize(Some(&pipe_name))?;
let client = Arc::new(client);                     // ← 必须 Arc<Mt5Client>

let req = TradeRequest {
    action: TRADE_ACTION_DEAL,
    symbol: "EURUSD".into(),
    volume: 0.10,
    r#type: ORDER_TYPE_BUY,
    ..Default::default()
};

client.order_send_async(&req, |result| match result {
    Ok(t)  => println!("下单完成：retcode={} price={}", t.retcode, t.price),
    Err(e) => println!("下单失败：{e}"),
})?;

// 调用方立即继续；回调在后台上线程里处理结果
```

**注意**：

- `order_send_async` 需要 `&Arc<Mt5Client>`（后台线程共享所有权）；
- 返回值 `Ok(())` 只代表**任务已派发**，不代表下单结果；
- **未初始化 / 连接已失效时不派发**，直接返回 `Err`（避免浪费线程）；
- 回调在后台线程执行，不要在里面做长时间阻塞；
- 每次调用 `spawn` 一个线程（跟单量级可接受；极高频场景建议在调用方用线程池收敛）。

---

## 6. API 参考

> 除特别注明外，均为 `Mt5Client` 的方法，返回 `Result<T>`，失败时返回 [`Mt5Error`](#10-错误处理)。

### 6.1 连接与生命周期

| 函数 | 说明 |
|---|---|
| `Mt5Client::new()` | 创建客户端（未连接） |
| `initialize(pipe_name)` | 建立连接 + 握手（默认 3s 超时） |
| `initialize_with_timeout(pipe_name, ms)` | 同上，自定义超时（`0` = 不启用） |
| `shutdown()` | 关闭连接 |
| `set_timeout_ms(ms)` / `timeout_ms()` | 动态调整 / 读取超时 |
| `is_connection_broken()` | 连接是否因超时失效（需重新 `initialize`） |
| `last_error()` | 最近一次调用的错误码与描述 |
| `send_raw_command(cmd, data)` | 发送任意原始命令（协议调试 / 自定义命令） |

### 6.2 账户、终端与登录

| 函数 | 说明 | 命令码 |
|---|---|---|
| `login(login, password, server)` | 登录账户（状态码 0 成功） | 4 |
| `account_info()` | 账户信息（余额/净值/保证金/杠杆等） | 190 |
| `terminal_info()` | 终端信息（连接状态/build/路径等） | 180 |
| `version()` | MT5 版本（复用 `terminal_info`） | 180 |

### 6.3 交易品种

| 函数 | 说明 | 命令码 |
|---|---|---|
| `symbols_total()` | 品种总数 | 173 |
| `symbols_get()` | 全部品种完整信息 | 174 |
| `symbol_info(symbol)` | 单品种信息（不存在返回 `Ok(None)`） | 170 |
| `symbol_info_tick(symbol)` | 最新报价 | 172 |
| `symbol_select(symbol, enable)` | 加入/移出市场报价 | 171 |

### 6.4 行情数据

| 函数 | 说明 | 命令码 |
|---|---|---|
| `copy_rates_from_pos(symbol, timeframe, start_pos, count)` | 从位置取 K 线 | 108 |
| `copy_rates_from(symbol, timeframe, date_from, count)` | 从日期取 K 线 | 106 |
| `copy_rates_range(symbol, timeframe, from, to)` | 区间 K 线 | 107 |
| `copy_ticks_from(symbol, from, count, flags)` | 从时间取 Tick | 104 |
| `copy_ticks_range(symbol, from, to, flags)` | 区间 Tick | 105 |

> `timeframe` 用 MT5 `TIMEFRAME_*` 值：1=M1、5=M5、15=M15、30=M30、60=H1、240=H4、1440=D1、10080=W1、43200=MN1。

### 6.5 持仓、订单与历史

| 函数 | 说明 | 命令码 |
|---|---|---|
| `positions_total()` / `positions_get(symbol)` | 持仓总数 / 列表 | 120 / 121 |
| `orders_total()` / `orders_get(symbol)` | 挂单总数 / 列表 | 130 / 131 |
| `history_deals_total(from, to)` / `history_deals_get(from, to)` | 成交统计 / 列表 | 150 / 151 |
| `history_orders_total(from, to)` / `history_orders_get(from, to)` | 订单历史统计 / 列表 | 140 / 141 |

> 时间参数均为 Unix 秒（含端点）。

### 6.6 市场深度

| 函数 | 说明 | 命令码 |
|---|---|---|
| `market_book_add(symbol)` | 订阅 DOM | 191 |
| `market_book_get(symbol)` | 读取档位 | 193 |
| `market_book_release(symbol)` | 取消订阅 | 192 |

### 6.7 交易计算与交易

| 函数 | 说明 | 命令码 |
|---|---|---|
| `order_calc_margin(action, symbol, volume, price)` | 所需保证金（本地计算） | — |
| `order_calc_profit(action, symbol, volume, price_open, price_close)` | 预期利润（本地计算） | — |
| `order_check(&TradeRequest)` | 请求预检（`retcode == 0` 通过） | 160 |
| `order_send(&TradeRequest)` | 同步下单 | 161 |
| `order_send_async(&TradeRequest, cb)` | **异步下单**（发出即返回，结果回调） | 161 |

### 6.8 终端发现（`protocol` 模块）

| 函数 | 说明 |
|---|---|
| `discover_mt5_pipe()` | 发现**一个**可连接终端（无则 **panic**） |
| `discover_all_mt5_pipes()` | 发现**全部**可连接终端的管道名（无则空 `Vec`） |
| `discover_all_terminals()` | 全部在线终端 → `Vec<(exe路径, 管道名)>` |
| `compute_pipe_name(terminal_path)` | 由终端路径算管道名（SHA-256） |
| `find_terminal64_paths()` | 枚举所有 `terminal64.exe` 路径 |
| `NamedPipeClient::with_timeout(pipe, ms)` | 底层管道客户端（消息模式 + 写锁 + 超时） |
| `DEFAULT_TIMEOUT_MS` | 默认超时常量（3000ms） |

### 6.9 数据结构与常量

| 结构体 | 说明 |
|---|---|
| `AccountInfo` / `TerminalInfo` / `VersionInfo` | 账户 / 终端 / 版本信息 |
| `SymbolInfo` / `Tick` / `Rate` | 品种 / 报价 / K 线 |
| `TradePosition` / `TradeOrder` / `TradeDeal` | 持仓 / 订单 / 成交 |
| `BookInfo` | 市场深度单档 |
| `TradeRequest`（实现 `Default`）/ `TradeResult` / `TradeCheckResult` | 交易请求 / 执行结果 / 检查结果 |

所有结构体**逐字段**都有中文注释，并对应 MQL5 官方结构体（`ACCOUNT_INFO`、`SYMBOL_INFO`、
`MqlRates`、`MqlTradeRequest` 等）语义。常用常量与 Python `MetaTrader5` 同名同值：

| 分类 | 常量 |
|---|---|
| `action` | `TRADE_ACTION_DEAL`(1)、`TRADE_ACTION_PENDING`(5)、`TRADE_ACTION_SLTP`(6)、`TRADE_ACTION_MODIFY`(7)、`TRADE_ACTION_REMOVE`(8)、`TRADE_ACTION_CLOSE_BY`(10) |
| `type` | `ORDER_TYPE_BUY`(0)、`ORDER_TYPE_SELL`(1)、`ORDER_TYPE_BUY_LIMIT`(2)、`ORDER_TYPE_SELL_LIMIT`(3)、`ORDER_TYPE_BUY_STOP`(4)、`ORDER_TYPE_SELL_STOP`(5)、`ORDER_TYPE_BUY_STOP_LIMIT`(6)、`ORDER_TYPE_SELL_STOP_LIMIT`(7) |
| `type_filling` | `ORDER_FILLING_FOK`(0)、`ORDER_FILLING_IOC`(1)、`ORDER_FILLING_RETURN`(2)、`ORDER_FILLING_BOC`(3) |
| `type_time` | `ORDER_TIME_GTC`(0)、`ORDER_TIME_DAY`(1)、`ORDER_TIME_SPECIFIED`(2)、`ORDER_TIME_SPECIFIED_DAY`(3) |
| `retcode` | `TRADE_RETCODE_OK`(0)、`TRADE_RETCODE_PLACED`(10008)、`TRADE_RETCODE_DONE`(10009)、`TRADE_RETCODE_REQUOTE`(10004)、`TRADE_RETCODE_NO_MONEY`(10019) 等 40 余个 |

---

## 7. 示例

| 示例 | 说明 | 运行 |
|---|---|---|
| `demo.rs` | 基础 API 演示（连接 → 账户/行情/持仓等只读查询） | `cargo run --example demo` |
| `pipe_features.rs` | **健壮性能力演示**：多终端发现 / 读取模式 / 并发保护 / 超时保护（真实触发）/ 异步下单接口 | `cargo run --example pipe_features` |
| `gui.rs` | API 调试 GUI（左侧全部接口列表，右侧中文参数表单，底部输出响应） | `cargo run --example gui --features gui` |

> `pipe_features` 的实测输出（本机 4 线程并发查询 20 次全部成功；超时用自建「假死管道」真实触发）：
>
> ```
> ③ 4 线程 × 5 次 = 20 次并发查询，成功 20 次，耗时 1ms
> ④ ✔ 捕获到超时（握手阶段）：[Timeout] 500ms 内未收到终端响应；连接已标记失效
>    error_code() = -10086   is_connection_broken() = true
>    ✔ 对真实终端重新 initialize 后恢复正常
> ```

> `gui.rs` 依赖 `eframe`，已改为**可选 feature**（`gui`）：不启用时构建库本体与其它示例都不会拉该依赖树。

---

## 8. 通信协议说明

与 Python `MetaTrader5` 库 / [go-mt5](https://github.com/Mukbeast4/go-mt5) 一致。

**请求帧**（客户端 → 终端）：

```
+----------------+----------------+------------------+
| 总长度 (u32 LE) | 命令码 (u32 LE) | 命令参数 (原始字节) |
+----------------+----------------+------------------+
```

- 「总长度」= 4（命令码字节数）+ 参数长度；
- 字符串参数编码：`[字符数 (u32 LE)] + [UTF-16LE 字符序列]`。

**响应帧**（终端 → 客户端）：

```
+----------------+----------------+------------------+------------------+
| 载荷长度 (u32 LE) | 命令码 (u32 LE) | 成功标志 (u32 LE) | 返回数据 (原始字节) |
+----------------+----------------+------------------+------------------+
```

- 载荷长度 < 8 视为非法响应；返回数据 = 载荷去掉 8 字节响应头；
- 数值一律小端序；字符串多为**固定宽度 UTF-16LE 槽**（遇 NUL 截断）。

**管道名计算**：

1. 终端路径 → 小写 + `\\?\` 前缀；
2. 按 UTF-16LE 编码为字节序列；
3. SHA-256 哈希；
4. 管道名 = `\\.\pipe\MT5.Terminal.<大写的十六进制哈希>`。

---

## 9. 实现说明

### 9.1 并发与超时的实现

- 写锁 `Arc<Mutex<()>>` 覆盖「`WriteFile` + `ReadFile`」整段；
- 启用超时时：后台线程执行上述整段（持锁），调用线程 `recv_timeout` 等待；
- 超时 → 置 `broken = true` → 后续请求立即返回错误。
  （后台线程若仍阻塞，会在对端进程退出时报错返回，不会永久泄漏。）

### 9.2 `order_calc_*` 采用本地计算

与 `go-mt5`（发送 202/203 命令）不同，本库与 Python `MetaTrader5` 一致使用本地公式：

- 保证金 = 交易量 × 价格 × `margin_initial` / 4；
- 利润 = 交易量 × (平仓价 − 开仓价) × `trade_contract_size`。

**原因**：MT5 Build 5836+ 通过 IPC 执行 202/203 会触发「管道已关闭」错误，本地计算可完全规避。

> ⚠️ 注意：这两个函数依赖 `symbol_info()` 成功返回；品种无效时内部 `unwrap()` 会 panic。

### 9.3 响应解析的容错

持仓/订单/成交/K线/Tick 等列表响应先读数量再逐条解析定长记录；
单条记录越界时**提前终止并返回已解析部分**（而非整体失败），以适应终端返回的细微差异。

### 9.4 交易请求/响应的二进制格式

- 请求：定长 **232 字节**（`action` → `magic`/`order` → `symbol`(64B UTF-16 槽) →
  `volume`/`price`/`stoplimit`/`sl`/`tp` → `deviation` → `type`/`type_filling`/`type_time` →
  `expiration` → `comment`(64B UTF-16 槽) → `position`/`position_by`）；字符串槽最多 31 字符；
- `order_send` 响应 **260 字节**（`retcode`/`deal`/`order`/`volume`/`price`/`bid`/`ask`/
  `comment`(200B 槽)/`request_id`/`retcode_external`）；
- `order_check` 响应 **252 字节**（无 `request_id`，与 Python 库输出一致）。

以上布局与 [go-mt5](https://github.com/Mukbeast4/go-mt5) 逐字节一致。

### 9.5 `copy_ticks_*` 的时间戳单位

终端命令 104/105 期望**毫秒**时间戳（与 MQL5 `CopyTicks` 一致），
本库 API 与 Python 库一致用**秒**，内部自动 ×1000。

> 上游/`go-mt5` 未做此转换，终端会把秒值当毫秒（≈1970 年），
> 导致 `copy_ticks_from` 返回最早期缓存、`copy_ticks_range` 返回空。**本分支已修正**。

---

## 10. 错误处理

统一返回 `Result<T, Mt5Error>`：

| 变体 | 触发场景 | 建议处理 |
|---|---|---|
| `IoError` | 管道读写失败等底层 IO 错误 | 重新 `initialize` |
| `ConnectionFailed` | 管道名缺失、管道打不开（终端未运行）、**连接已失效** | 检查终端 / 重新 `initialize` |
| `ProtocolError` | 协议层错误（如进程快照失败） | 记录并重试 |
| `InvalidResponse` | 响应过短、载荷非法 | 记录；确认终端版本兼容 |
| `NotInitialized` | 未 `initialize` 就调用其它 API | 先建连接 |
| `CommandFailed` | 终端返回非零状态码（含 `error_code`，如下单 `10030`） | 按错误码处理业务 |
| `Timeout` | **请求超时；连接已标记失效** | 记录日志 → 重新 `initialize` |
| `NotSupported` | 功能暂不支持（当前全部 32 个 API 已实现，保留备用） | — |

辅助方法：`error_code()`（数值码；`Timeout` 返回 `-10086`）、`is_connection_lost()`。

---

## 11. 系统要求

- Rust 2021 edition（`rust-version = 1.70`）；
- **Windows** 操作系统（命名管道 IPC 为 Windows 专属机制）；
- MT5 终端正在运行且已登录账户。

---

## 12. 许可证与出处

MIT。

- 上游：[`yjkt/mt5-rs`](https://github.com/yjkt/mt5-rs)，Copyright (c) 2026 yjkt；
- 本分支：`Mt5-rs-Core`，Copyright (c) 2026 Anjou（增强与维护）。

依 MIT 要求，原始版权声明与许可声明保留于 `LICENSE` 文件中。
本分支在 [§1](#1-相对上游的增强) 列出的改动之外，API 命名与协议实现保持与上游/ Python `MetaTrader5` 兼容。
