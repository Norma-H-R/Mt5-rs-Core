//! # 四项健壮性能力演示
//!
//! 运行方式：
//!
//! ```text
//! cargo run --example pipe_features
//! ```
//!
//! 本示例依次演示 `protocol` 层提供的四项保障（详见 `src/protocol.rs` 模块文档）：
//!
//! | # | 能力 | 演示内容 |
//! |---|---|---|
//! | 1 | **多终端发现** | 枚举本机全部在线 MT5 终端，拿到 `terminal64.exe` 路径 + 管道名 |
//! | 2 | **管道读取模式** | 连接即声明 `PIPE_READMODE_MESSAGE`（消息模式），避免 `WriteFile` 永久阻塞 |
//! | 3 | **并发保护** | 同一个 `Mt5Client` 放进 `Arc`，多线程并发查询（内部写锁串行化，不串包） |
//! | 4 | **超时保护** | 自建「假死管道」真实触发超时 → 连接标记失效 → 重新 `initialize` 恢复 |
//! | 5 | **异步下单** | `order_send_async` 的派发语义（发出即返回、结果回调）——仅接口形态，不真实下单 |
//!
//! 前提：本机至少有一个 MT5 终端在运行并已登录账户；否则示例会打印提示后正常退出。
//!
//! > 注意：示例**只做只读查询**（账户信息），不会下单。第 ④ 项的「假死管道」
//! > 是本示例自建的本地命名管道（只接连接、永不回应），不对应任何真实终端。

use mt5_rs::{
    discover_all_mt5_pipes, discover_all_terminals, Mt5Client, Mt5Error, TradeRequest,
    DEFAULT_TIMEOUT_MS,
};
use std::sync::Arc;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::PIPE_ACCESS_DUPLEX;
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_MESSAGE, PIPE_TYPE_MESSAGE,
};

/// 演示「超时保护」用的假死管道名（本地自建，与 MT5 无关）。
const DEAF_PIPE: &str = r"\\.\pipe\mt5_rs_deaf_pipe";

fn main() {
    banner("① 多终端发现（discover_all_terminals / discover_all_mt5_pipes）");

    // ── 1. 多终端发现 ────────────────────────────────────────────────
    // 枚举所有 terminal64.exe → 计算管道名 → 只保留当前可连接的。
    let terminals = discover_all_terminals();

    if terminals.is_empty() {
        println!("未发现在线 MT5 终端（枚举逻辑正常，返回空列表且不 panic）。");
        println!("请先启动 MT5 终端并登录账户，然后重新运行本示例以查看完整演示。");
        println!("下面仍可演示第 ⑤ 项（异步下单接口的派发语义，不需要真实终端）：");
        demo_async_dispatch();
        return;
    }

    println!("发现 {} 个在线 MT5 终端：", terminals.len());
    for (i, (exe_path, pipe_name)) in terminals.iter().enumerate() {
        println!("  [{i}] exe  : {exe_path}");
        println!("      pipe : {pipe_name}");
    }

    // 只取管道名的版本（多实例批量连接时更顺手）
    let pipes_only = discover_all_mt5_pipes();
    println!("  discover_all_mt5_pipes() = {pipes_only:?}");

    // 本示例只连第一个终端
    let pipe = terminals[0].1.clone();

    // ── 2. 管道读取模式 ─────────────────────────────────────────────
    banner("② 管道读取模式（PIPE_READMODE_MESSAGE，连接时自动完成）");

    let mut client = Mt5Client::new();
    match client.initialize(Some(&pipe)) {
        Ok(()) => println!(
            "已连接 {pipe}\n\
             （NamedPipeClient::new 内部已调用 SetNamedPipeHandleState(PIPE_READMODE_MESSAGE)；\n\
              MT5 是消息模式管道，不声明按消息读取会导致 WriteFile 永久阻塞）"
        ),
        Err(e) => {
            println!("连接失败：{e}");
            return;
        }
    }

    // ── 3. 并发保护 ─────────────────────────────────────────────────
    banner("③ 并发保护（Arc<Mt5Client> 多线程并发查询）");

    // Mt5Client: Send + Sync —— 内部有写锁，可被多个线程共享；
    // 任意时刻只有一个「写请求 + 读响应」在执行，因此响应不会与请求错配。
    let shared = Arc::new(client);
    const THREADS: usize = 4;
    const ROUNDS: usize = 5;

    let started = Instant::now();
    let handles: Vec<_> = (0..THREADS)
        .map(|tid| {
            let c = Arc::clone(&shared);
            std::thread::spawn(move || {
                let mut ok = 0usize;
                for round in 0..ROUNDS {
                    match c.account_info() {
                        Ok(acc) => {
                            ok += 1;
                            // 只在「0 号线程第 1 轮」打印一次，避免刷屏
                            if tid == 0 && round == 0 {
                                println!(
                                    "  [线程 {tid}] 第 1 次查询成功：login={} balance={:.2}",
                                    acc.login, acc.balance
                                );
                            }
                        }
                        Err(e) => println!("  [线程 {tid}] 查询失败：{e}"),
                    }
                }
                ok
            })
        })
        .collect();

    let total_ok: usize = handles.into_iter().map(|h| h.join().unwrap_or(0)).sum();
    println!(
        "  {THREADS} 线程 × {ROUNDS} 次 = {} 次并发查询，成功 {total_ok} 次，耗时 {}ms",
        THREADS * ROUNDS,
        started.elapsed().as_millis()
    );
    println!("  说明：请求被内部写锁串行化，每次响应都与自己的请求一一对应（无串包）。");

    // 拿回所有权，便于后面的超时演示
    let client = Arc::try_unwrap(shared).unwrap_or_else(|_| panic!("Arc 仍被占用"));
    println!(
        "  当前超时配置 = {:?} ms（默认 {DEFAULT_TIMEOUT_MS} ms）",
        client.timeout_ms()
    );

    // ── 4. 超时保护 ─────────────────────────────────────────────────
    banner("④ 超时保护（真实触发：自建「假死管道」，只接连接、永不回应）");

    // 4.0 起一个本地假死命名管道服务端（模拟「终端假死 / 管道半连接」）
    spawn_deaf_pipe();
    std::thread::sleep(Duration::from_millis(100)); // 等服务端就绪
    println!("  已启动假死管道 {DEAF_PIPE}（接受连接，但不回应任何请求）");

    // 4.1 用 500ms 超时连接它：握手阶段就会超时
    let mut deaf = Mt5Client::new();
    match deaf.initialize_with_timeout(Some(DEAF_PIPE), 500) {
        Err(Mt5Error::Timeout(msg)) => {
            println!("  ✔ 捕获到超时（握手阶段）：[Timeout] {msg}");
            println!(
                "    error_code() = {}（本库自定义超时码）",
                Mt5Error::Timeout(String::new()).error_code()
            );
            println!(
                "    is_connection_lost() = {}",
                Mt5Error::Timeout(String::new()).is_connection_lost()
            );
        }
        Err(e) => println!("  得到其它错误：{e}"),
        Ok(()) => println!("  意外：假死管道竟然完成了握手"),
    }

    // 4.2 超时后连接被标记失效：后续调用立即失败，不再往残破管道上读写
    println!(
        "  连接是否已标记失效 is_connection_broken() = {}",
        deaf.is_connection_broken()
    );
    match deaf.account_info() {
        Err(e) => println!("  失效后再次调用（立即失败，不会卡住）：{e}"),
        Ok(_) => println!("  意外：已失效的连接仍然可用"),
    }

    // 4.3 恢复路径：对**真实终端**重新 initialize 即可
    let mut c = client;
    match c.initialize(Some(&pipe)) {
        Ok(()) => {
            let _ = c.set_timeout_ms(DEFAULT_TIMEOUT_MS);
            match c.account_info() {
                Ok(acc) => println!(
                    "  ✔ 对真实终端重新 initialize 后恢复正常：login={} balance={:.2}（超时 {} ms）",
                    acc.login, acc.balance, DEFAULT_TIMEOUT_MS
                ),
                Err(e) => println!("  恢复后查询失败：{e}"),
            }
        }
        Err(e) => println!("  重新 initialize 失败：{e}"),
    }

    // ── 5. 异步下单接口（只演示派发语义，不真实下单）────────────────
    banner("⑤ 异步下单接口（order_send_async：发出即返回，结果回调）");
    demo_async_dispatch();

    // ── 收尾 ────────────────────────────────────────────────────────
    banner("演示结束");
    println!("能力小结：");
    println!("  1. 多终端发现 —— discover_all_terminals() / discover_all_mt5_pipes()");
    println!("  2. 读取模式   —— 连接时自动设置 PIPE_READMODE_MESSAGE");
    println!("  3. 并发保护   —— Arc<Mt5Client> 多线程共享，内部写锁串行化");
    println!("  4. 超时保护   —— set_timeout_ms() 调整；超时后连接失效，重新 initialize 恢复");
    println!("  5. 异步下单   —— Arc<Mt5Client>::order_send_async()，发出即返回、结果回调");
}

/// 打印带分隔线的标题。
fn banner(title: &str) {
    println!("\n──────── {title} ────────");
}

/// 演示异步下单接口的**派发语义**（不真实下单）。
///
/// 用「未初始化」的客户端调用，必然被派发前置检查拒绝——
/// 这样既能展示 `Arc<Mt5Client>::order_send_async` 的调用形态，
/// 又不会真的下出任何单子。
fn demo_async_dispatch() {
    let idle = Arc::new(Mt5Client::new());
    match idle.order_send_async(&TradeRequest::default(), |_r| {}) {
        Err(e) => println!("  未初始化时派发被拒绝（不会空转线程）：{e}"),
        Ok(()) => println!("  意外：未初始化竟然派发成功"),
    }
    println!("  真实用法（详见 README「下单专题」）：");
    println!("    let c = Arc::new(client);                       // 必须是 Arc<Mt5Client>");
    println!("    c.order_send_async(&req, |result| match result {{");
    println!("        Ok(t)  => println!(\"下单完成 retcode={{}}\", t.retcode),");
    println!("        Err(e) => println!(\"下单失败: {{e}}\"),");
    println!("    }})?;                                            // 调用方立即继续");
}

/// 起一个「只接受连接、永不回应」的本地命名管道服务端，用于**真实触发超时**。
///
/// 用真实终端很难稳定演示超时（本机响应常在亚毫秒级），
/// 因此这里自建一条管道：接受连接后既不读也不写，
/// 客户端发完请求后会一直等不到响应，从而走到超时分支。
///
/// 该线程阻塞在 `ConnectNamedPipe`；示例结束随进程退出，无需清理。
fn spawn_deaf_pipe() {
    std::thread::spawn(|| unsafe {
        let name: Vec<u16> = DEAF_PIPE
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();

        // 消息模式管道（与 MT5 同类），双工、单实例、64KB 缓冲
        let handle = CreateNamedPipeW(
            name.as_ptr(),
            PIPE_ACCESS_DUPLEX,
            PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE,
            1,
            64 * 1024,
            64 * 1024,
            0,
            std::ptr::null(),
        );
        if handle == INVALID_HANDLE_VALUE {
            println!("  （假死管道创建失败，超时演示将退化为无终端时的行为）");
            return;
        }

        // 等一个客户端连上来；随后故意不读不写 → 客户端等响应超时
        ConnectNamedPipe(handle, std::ptr::null_mut());
        std::thread::sleep(Duration::from_secs(15));
        CloseHandle(handle);
    });
}
