# Naviscope 并发安全分析

## 概述

本文档分析 Naviscope 中 LSP、Shell 和 MCP 三个组件的并发安全性，识别潜在的死锁风险并提供改进建议。

**分析日期**: 2026-01-30  
**结论**: ✅ 当前代码不存在明显的死锁风险，但存在性能和可维护性方面的改进空间。

---

## 架构概览

### 组件和锁的使用

| 组件 | 运行时 | 共享状态 | 锁类型 |
|------|--------|----------|--------|
| **LSP Server** | Async (tokio) | `Arc<tokio::sync::RwLock<Option<Naviscope>>>` | 异步读写锁 |
| **MCP Server** | Async (tokio) | 与 LSP 共享同一个 Arc | 异步读写锁 |
| **Shell REPL** | Sync (独立进程) | `Arc<std::sync::RwLock<Naviscope>>` | 同步读写锁 |

### 关键共享路径

```
LSP Server
    ├─ Indexer Task (写者)
    │   └─ engine.write().await → 重建索引 → 更新 Arc<RwLock<Option<Naviscope>>>
    │
    └─ MCP HTTP Server (读者)
        └─ engine.read().await → 克隆引擎 → 执行查询

Shell REPL (独立进程)
    ├─ Watcher Thread (写者)
    │   └─ engine.write() → refresh() → 更新 Arc<RwLock<Naviscope>>
    │
    ├─ Completer (读者)
    │   └─ engine.read() → 计算补全建议
    │
    └─ Command Handlers (读者)
        └─ engine.read() → 执行查询
```

---

## 死锁风险分析

### ✅ 风险1：LSP/MCP 的锁使用模式

**代码位置**: `src/lsp/indexer.rs`, `src/mcp/mod.rs`

**模式**:
```rust
// LSP Indexer (写者)
let mut lock = engine_lock.write().await;
*lock = Some(navi.clone());
// lock 在此处自动释放

// MCP Server (读者)
async fn get_or_build_index(&self) -> Result<Naviscope, McpError> {
    let lock = self.engine.read().await;
    match &*lock {
        Some(navi) => Ok(navi.clone()),  // 克隆后立即释放锁
        None => Err(...)
    }
}  // lock 在此处自动释放

async fn execute_query(&self, query: GraphQuery) -> ... {
    let engine = self.get_or_build_index().await?;  // 锁已释放
    tokio::task::spawn_blocking(move || {
        // 使用克隆的 engine，不持有原始锁
        ...
    }).await
}
```

**分析**:
- ✅ **安全**: 读锁和写锁都在短时间内释放
- ✅ **正确的模式**: "获取锁 → 克隆数据 → 释放锁 → 使用克隆"
- ⚠️ **性能问题**: 克隆 `Naviscope` 的成本（需要确认是廉价克隆）
- ⚠️ **写锁阻塞**: indexer 重建期间，MCP 查询会被阻塞

**建议**: 
1. 确认 `Naviscope::clone()` 的实现（是否是 `Arc` 包装的廉价克隆）
2. 考虑使用"双缓冲"或"MVCC"模式，允许查询使用旧版本索引

---

### ✅ 风险2：Shell 与 LSP/MCP 的隔离

**代码位置**: `src/cli/shell/mod.rs`

**模式**:
```rust
// Shell (独立进程)
pub struct ReplServer {
    context: ShellContext,  // 包含 Arc<std::sync::RwLock<Naviscope>>
}
```

**分析**:
- ✅ **完全隔离**: Shell 是独立进程，使用自己的 `RwLock`
- ✅ **无跨进程锁**: 不存在与 LSP/MCP 的锁竞争
- ⚠️ **混合锁类型**: 如果未来在同一进程中运行，会有问题（`std::sync::RwLock` vs `tokio::sync::RwLock`）

**建议**: 
1. 在文档中明确说明 Shell 必须作为独立进程运行
2. 如果考虑嵌入式 Shell，需要统一使用 `tokio::sync::RwLock`

---

### ⚠️ 风险3：Shell Completer 的读锁持有时间

**代码位置**: `src/cli/shell/completer.rs:62-149`

**问题**:
```rust
if let Ok(naviscope) = self.context.naviscope.read() {
    let graph = naviscope.graph();
    
    // ⚠️ 在读锁持有期间执行大量计算
    let matches: Vec<String> = graph.fqn_map.keys()
        .filter(|fqn| fqn.starts_with(last_word))
        .take(20)
        .cloned()
        .collect();
    
    // ... 更多计算 ...
    
    suggestions.sort_by(...);  // 排序操作
    suggestions.truncate(50);
    
    return suggestions;
}  // 读锁在此处才释放
```

**分析**:
- ⚠️ **读锁持有时间过长**: 整个补全计算期间都持有读锁
- ⚠️ **阻塞写者**: 如果 watcher 尝试更新索引，会被阻塞
- ✅ **不是死锁**: RwLock 保证最终会释放，只是性能问题

**影响**:
- 用户按下 Tab 键时，如果补全计算耗时较长（比如大型项目），会阻止索引更新
- 反之，索引更新时，补全会等待

**建议**:
```rust
// 改进版本：缩短锁持有时间
let (graph_snapshot, fqn_map_keys) = {
    let naviscope = self.context.naviscope.read()?;
    let graph = naviscope.graph();
    
    // 仅在锁内收集必要的数据
    let keys: Vec<String> = graph.fqn_map.keys()
        .filter(|fqn| fqn.starts_with(last_word))
        .take(20)
        .cloned()
        .collect();
    
    (graph.clone(), keys)  // 或使用 Arc
};  // 读锁在此处释放

// 在锁外执行耗时计算
let mut suggestions = Vec::new();
for fqn in fqn_map_keys {
    suggestions.push(Suggestion { ... });
}
suggestions.sort_by(...);
```

---

### ⚠️ 风险4：Shell Watcher 的嵌套调用

**代码位置**: `src/cli/shell/mod.rs:146-160`

**问题**:
```rust
match naviscope_clone.write() {
    Ok(mut engine) => {
        // ⚠️ 在持有写锁期间调用 refresh()
        if let Err(e) = engine.refresh() {
            error!("Error during re-indexing: {}", e);
        } else {
            // ⚠️ 调用 graph()
            let index = engine.graph();
            info!("Indexing complete! Nodes: {}, Edges: {}",
                index.topology.node_count(),
                index.topology.edge_count()
            );
        }
    }
    Err(e) => error!("Failed to acquire lock for re-indexing: {}", e),
}
```

**潜在风险**:
- 如果 `Naviscope::refresh()` 或 `Naviscope::graph()` 内部尝试获取其他锁：
  - 可能导致嵌套锁定（如果它们尝试获取同一个锁）
  - 可能导致锁顺序不一致（如果存在多个锁）

**检查清单**:
- [ ] 确认 `Naviscope::refresh()` 不尝试获取 `RwLock`
- [ ] 确认 `Naviscope::graph()` 是简单的 getter，不涉及锁
- [ ] 确认 `Naviscope` 内部没有其他互斥锁

**建议**:
```rust
// 如果 refresh() 可能耗时很长，考虑释放锁后执行
let mut engine = naviscope_clone.write().unwrap();
drop(engine);  // 显式释放写锁

// 在锁外执行重索引
let mut temp_engine = Naviscope::new(path.clone());
if let Err(e) = temp_engine.refresh() {
    error!("Error during re-indexing: {}", e);
    return;
}

// 原子替换
*naviscope_clone.write().unwrap() = temp_engine;
```

---

### ⚠️ 风险5：LSP Indexer 长时间持有写锁

**代码位置**: `src/lsp/indexer.rs:102-111`

**问题**:
```rust
let (res, n) = {
    let mut n = navi;
    tokio::task::spawn_blocking(move || {
        let res = n.build_index();  // ⚠️ 可能耗时数秒到数分钟
        (res, n)
    })
    .await
    .expect("Indexer task panicked")
};
navi = n;

// 发布更新
let mut lock = engine_lock.write().await;  // ⚠️ 获取写锁
*lock = Some(navi.clone());  // 克隆并更新
// 锁自动释放
```

**分析**:
- ✅ **当前是安全的**: 索引构建在锁外执行，只在最后获取写锁更新
- ⚠️ **但写锁期间会阻塞所有 MCP 查询**: 虽然时间很短（只是克隆和赋值），但仍会导致短暂的服务不可用

**影响**:
- 在大型项目上，`build_index()` 可能耗时很长（虽然在锁外）
- 写锁期间，MCP 查询会被阻塞（虽然时间很短）

**建议**:
1. 使用"swap"模式，减少写锁持有时间
2. 考虑使用 `Arc` 包装 `Naviscope`，通过原子指针交换实现无锁更新

```rust
// 建议的改进
let new_navi = Arc::new(navi);  // 包装为 Arc

{
    let mut lock = engine_lock.write().await;
    *lock = Some(Arc::clone(&new_navi));  // 廉价的 Arc 克隆
}  // 写锁立即释放
```

---

## 锁定顺序分析

### 当前锁定路径

**LSP/MCP**:
```
engine_lock (RwLock) → Naviscope 内部状态（无额外锁）
```

**Shell**:
```
naviscope (RwLock) → Naviscope 内部状态（无额外锁）
current_node (RwLock) → 独立锁，无依赖
```

### 潜在的锁顺序问题

目前看起来不存在多个锁的嵌套，因此**不会因为锁顺序不一致导致死锁**。

但需要确认：
- [ ] `Naviscope` 内部是否有其他互斥锁？
- [ ] `QueryEngine` 是否持有锁？
- [ ] `CodeGraph` 是否有内部同步机制？

---

## 建议的改进措施

### 短期改进（低风险）

1. **缩短 Completer 的读锁持有时间**
   ```rust
   // 在锁内仅收集必要数据，在锁外计算
   let data = { naviscope.read()?.extract_data() };
   compute_suggestions(data);
   ```

2. **添加锁监控日志**
   ```rust
   let start = Instant::now();
   let lock = engine.write().await;
   if start.elapsed() > Duration::from_millis(100) {
       warn!("Write lock acquisition took {:?}", start.elapsed());
   }
   ```

3. **文档化锁的使用规则**
   - 在代码注释中明确说明锁的持有时间要求
   - 在 `CODING_STYLE.md` 中添加并发安全指南

### 中期改进（中等风险）

4. **使用 Arc 包装 Naviscope**
   ```rust
   type EngineRef = Arc<tokio::sync::RwLock<Option<Arc<Naviscope>>>>;
   ```
   - 好处：克隆成本降低，写锁持有时间缩短
   - 风险：需要修改多处代码

5. **实现"双缓冲"或 MVCC 模式**
   ```rust
   struct IndexStore {
       current: Arc<Naviscope>,
       building: Option<Arc<Naviscope>>,
   }
   ```
   - 好处：查询可以使用旧版本，不被索引重建阻塞
   - 风险：内存使用增加，复杂性提升

### 长期改进（高风险，需要架构变更）

6. **引入无锁数据结构**
   - 使用 `crossbeam` 或 `parking_lot` 的无锁/细粒度锁结构
   - 对 `CodeGraph` 的读取使用原子引用计数

7. **实现增量索引**
   - 避免全量重建，只更新变更部分
   - 减少写锁持有时间

8. **查询缓存层**
   - 缓存常见查询结果
   - 减少对索引的访问频率

---

## 测试建议

### 并发压力测试

```rust
#[tokio::test]
async fn test_concurrent_read_write() {
    let engine = Arc::new(RwLock::new(Some(Naviscope::new(...))));
    
    // 模拟 MCP 查询
    let readers: Vec<_> = (0..10)
        .map(|_| {
            let e = engine.clone();
            tokio::spawn(async move {
                for _ in 0..100 {
                    let lock = e.read().await;
                    // 模拟查询
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
        })
        .collect();
    
    // 模拟 LSP indexer
    let writer = {
        let e = engine.clone();
        tokio::spawn(async move {
            for _ in 0..5 {
                tokio::time::sleep(Duration::from_millis(50)).await;
                let mut lock = e.write().await;
                *lock = Some(Naviscope::new(...));
            }
        })
    };
    
    // 等待所有任务完成
    for r in readers {
        r.await.unwrap();
    }
    writer.await.unwrap();
}
```

### 死锁检测

考虑使用 `parking_lot` 的 deadlock detection 功能：

```rust
#[cfg(debug_assertions)]
use parking_lot::deadlock;

#[cfg(debug_assertions)]
std::thread::spawn(move || {
    loop {
        std::thread::sleep(Duration::from_secs(10));
        let deadlocks = deadlock::check_deadlock();
        if deadlocks.is_empty() {
            continue;
        }
        for (i, threads) in deadlocks.iter().enumerate() {
            error!("Deadlock #{}", i);
            for t in threads {
                error!("Thread Id {:#?}", t.thread_id());
                error!("{:#?}", t.backtrace());
            }
        }
    }
});
```

---

## 结论

**当前状态**: ✅ 无明显死锁风险

**主要问题**:
1. ⚠️ 性能问题：长时间持有读锁（completer）
2. ⚠️ 阻塞问题：索引重建时阻塞查询
3. ⚠️ 维护风险：未来代码变更可能引入死锁

**优先级建议**:
1. **立即**: 添加锁监控日志，识别实际瓶颈
2. **近期**: 优化 completer 的锁持有时间
3. **中期**: 考虑使用 Arc 包装减少克隆成本
4. **长期**: 评估 MVCC 或增量索引的可行性

**行动项**:
- [ ] 检查 `Naviscope::refresh()` 和 `Naviscope::graph()` 的实现
- [ ] 确认 `Naviscope::clone()` 的成本
- [ ] 添加并发压力测试
- [ ] 在 CI 中启用死锁检测工具
- [ ] 更新 `CODING_STYLE.md`，添加并发安全指南
