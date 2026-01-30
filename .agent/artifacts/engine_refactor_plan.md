---
description: Naviscope Engine Refactor Implementation Plan
artifact_type: implementation_plan
---

# Naviscope 引擎重构实施计划

**目标**: 重构索引引擎，使用 Arc + CoW + MVCC 架构，支持 LSP/MCP/Shell 多端高效共用

**开始日期**: 2026-01-31  
**预计完成**: 2026-02-10 (10 天)  
**状态**: 🚧 准备阶段

---

## 📋 阶段概览

- [x] **阶段 0**: 准备工作 (完成)
- [ ] **阶段 1**: 核心引擎实现 (2-3 天)
- [ ] **阶段 2**: LSP 迁移 (2 天)
- [ ] **阶段 3**: MCP 迁移 (1 天)
- [ ] **阶段 4**: Shell 迁移 (1 天)
- [ ] **阶段 5**: 测试与优化 (2 天)
- [ ] **阶段 6**: 清理与发布 (1 天)

---

## ✅ 阶段 0: 准备工作

### 文档准备
- [x] 创建并发安全分析文档 (`CONCURRENCY_ANALYSIS.md`)
- [x] 创建重构设计方案 (`REFACTOR_ENGINE.md`)
- [x] 创建架构对比文档 (`REFACTOR_COMPARISON.md`)
- [x] 创建原型代码 (`engine_prototype.rs`)

### 环境准备
- [ ] 创建新分支 `refactor/unified-engine`
- [ ] 备份当前代码状态
- [ ] 运行完整测试套件，确保基线正常

---

## 🏗️ 阶段 1: 核心引擎实现 (Day 1-3)

### Task 1.1: 创建引擎模块结构 ⏱️ 30分钟

- [ ] 创建 `src/engine/` 目录
- [ ] 创建 `src/engine/mod.rs`
- [ ] 创建 `src/engine/graph.rs` (Arc 包装的 CodeGraph)
- [ ] 创建 `src/engine/builder.rs` (CodeGraphBuilder)
- [ ] 创建 `src/engine/engine.rs` (NaviscopeEngine)
- [ ] 创建 `src/engine/handle.rs` (EngineHandle)
- [ ] 在 `src/lib.rs` 中导出 `engine` 模块

**验证标准**: 
```bash
cargo build --lib
# 应该能编译通过（即使模块是空的）
```

---

### Task 1.2: 实现 Arc 包装的 CodeGraph ⏱️ 2-3小时

**文件**: `src/engine/graph.rs`

- [ ] 定义 `CodeGraphInner` 结构体
  ```rust
  struct CodeGraphInner {
      version: u32,
      topology: StableDiGraph<GraphNode, GraphEdge>,
      fqn_map: HashMap<String, NodeIndex>,
      name_map: HashMap<String, Vec<NodeIndex>>,
      file_map: HashMap<PathBuf, SourceFile>,
      path_to_nodes: HashMap<PathBuf, Vec<NodeIndex>>,
  }
  ```

- [ ] 定义 `CodeGraph` 包装结构
  ```rust
  #[derive(Clone)]
  pub struct CodeGraph {
      inner: Arc<CodeGraphInner>,
  }
  ```

- [ ] 实现构造函数
  - [ ] `CodeGraph::empty()` - 创建空图
  - [ ] `CodeGraph::from_inner(inner: CodeGraphInner)` - 从内部结构创建

- [ ] 实现只读访问方法
  - [ ] `find_node(&self, fqn: &str) -> Option<NodeIndex>`
  - [ ] `get_node(&self, idx: NodeIndex) -> Option<&GraphNode>`
  - [ ] `find_node_at(&self, path: &Path, line: usize, col: usize) -> Option<NodeIndex>`
  - [ ] `find_matches_by_fqn(&self, fqn: &str) -> Vec<NodeIndex>`
  - [ ] `topology(&self) -> &StableDiGraph<...>` - 获取拓扑图引用
  - [ ] `fqn_map(&self) -> &HashMap<String, NodeIndex>` - 获取 FQN 映射
  - [ ] `file_map(&self) -> &HashMap<PathBuf, SourceFile>` - 获取文件映射

- [ ] 实现转换方法
  - [ ] `to_builder(&self) -> CodeGraphBuilder` - 创建构建器（深拷贝）

- [ ] 实现序列化支持
  - [ ] 为 `CodeGraphInner` 添加 `Serialize/Deserialize`
  - [ ] 实现 `load_from_disk(path: &Path) -> Result<Option<Self>>`
  - [ ] 实现 `save_to_disk(&self, path: &Path) -> Result<()>`

**验证标准**:
```bash
cargo test --lib engine::graph::tests
# 测试 Arc clone 的性能
# 测试序列化/反序列化
```

**测试用例**:
- [ ] `test_arc_clone_is_cheap()` - 验证克隆是 O(1)
- [ ] `test_immutability()` - 验证图是不可变的
- [ ] `test_serialization()` - 验证可以序列化和反序列化

---

### Task 1.3: 实现 CodeGraphBuilder ⏱️ 2-3小时

**文件**: `src/engine/builder.rs`

- [ ] 定义 `CodeGraphBuilder` 结构体
  ```rust
  pub struct CodeGraphBuilder {
      inner: CodeGraphInner,  // 可变的内部数据
  }
  ```

- [ ] 实现构造函数
  - [ ] `new() -> Self` - 创建新构建器
  - [ ] `from_graph(graph: &CodeGraph) -> Self` - 从现有图创建（用于增量更新）

- [ ] 实现图操作方法
  - [ ] `add_node(&mut self, fqn: String, node: GraphNode) -> NodeIndex`
  - [ ] `add_edge(&mut self, from: NodeIndex, to: NodeIndex, edge: GraphEdge)`
  - [ ] `remove_node(&mut self, idx: NodeIndex)`
  - [ ] `remove_path(&mut self, path: &PathBuf)`
  - [ ] `update_file(&mut self, path: PathBuf, source: SourceFile)`

- [ ] 实现批量操作
  - [ ] `apply_op(&mut self, op: GraphOp)` - 应用单个图操作
  - [ ] `apply_ops(&mut self, ops: Vec<GraphOp>)` - 批量应用操作

- [ ] 实现构建方法
  - [ ] `build(self) -> CodeGraph` - 完成构建，返回不可变图

**验证标准**:
```bash
cargo test --lib engine::builder::tests
```

**测试用例**:
- [ ] `test_build_from_scratch()` - 从零构建图
- [ ] `test_incremental_update()` - 增量更新现有图
- [ ] `test_remove_operations()` - 删除节点和路径

---

### Task 1.4: 实现 NaviscopeEngine ⏱️ 3-4小时

**文件**: `src/engine/engine.rs`

- [ ] 定义 `NaviscopeEngine` 结构体
  ```rust
  pub struct NaviscopeEngine {
      current: Arc<RwLock<Arc<CodeGraph>>>,
      project_root: PathBuf,
      index_path: PathBuf,
  }
  ```

- [ ] 实现构造函数
  - [ ] `new(project_root: PathBuf) -> Self`
  - [ ] 自动计算 `index_path`（使用哈希）

- [ ] 实现快照方法
  - [ ] `async fn snapshot(&self) -> CodeGraph`
    - 获取读锁
    - Arc clone 当前图
    - 立即释放锁

- [ ] 实现索引加载
  - [ ] `async fn load(&self) -> Result<bool>`
    - 在 blocking pool 加载磁盘索引
    - 原子更新 current
    - 返回是否成功加载

- [ ] 实现索引保存
  - [ ] `async fn save(&self) -> Result<()>`
    - 获取当前快照
    - 在 blocking pool 保存到磁盘

- [ ] 实现索引重建
  - [ ] `async fn rebuild(&self) -> Result<()>`
    - 在 blocking pool 扫描和解析
    - 构建新图
    - 原子更新 current
    - 保存到磁盘

- [ ] 实现增量更新
  - [ ] `async fn update_files(&self, files: Vec<PathBuf>) -> Result<()>`
    - 获取当前图的构建器
    - 重新解析变更文件
    - 更新构建器
    - 构建新图并更新

- [ ] 实现刷新方法
  - [ ] `async fn refresh(&self) -> Result<()>`
    - 检测文件变更
    - 调用 update_files 或 rebuild

**验证标准**:
```bash
cargo test --lib engine::engine::tests
```

**测试用例**:
- [ ] `test_snapshot_is_fast()` - 验证快照获取性能
- [ ] `test_rebuild_updates_index()` - 验证重建功能
- [ ] `test_incremental_update()` - 验证增量更新
- [ ] `test_concurrent_snapshots()` - 验证并发快照
- [ ] `test_load_save_roundtrip()` - 验证持久化

---

### Task 1.5: 实现 EngineHandle ⏱️ 2小时

**文件**: `src/engine/handle.rs`

- [ ] 定义 `EngineHandle` 结构体
  ```rust
  #[derive(Clone)]
  pub struct EngineHandle {
      engine: Arc<NaviscopeEngine>,
  }
  ```

- [ ] 实现构造函数
  - [ ] `new(project_root: PathBuf) -> Self`

- [ ] 实现异步接口（用于 LSP/MCP）
  - [ ] `async fn graph(&self) -> CodeGraph`
  - [ ] `async fn query(&self, query: &GraphQuery) -> Result<QueryResult>`
  - [ ] `async fn rebuild(&self) -> Result<()>`
  - [ ] `async fn load(&self) -> Result<bool>`
  - [ ] `async fn save(&self) -> Result<()>`

- [ ] 实现同步接口（用于 Shell）
  - [ ] `fn graph_blocking(&self) -> CodeGraph`
  - [ ] `fn query_blocking(&self, query: &GraphQuery) -> Result<QueryResult>`

- [ ] 实现文件监听
  - [ ] `async fn watch(&self) -> Result<()>`
    - 启动后台任务监听文件变更
    - 调用 engine.refresh()

**验证标准**:
```bash
cargo test --lib engine::handle::tests
```

**测试用例**:
- [ ] `test_async_graph_access()` - 测试异步接口
- [ ] `test_blocking_graph_access()` - 测试同步接口
- [ ] `test_concurrent_queries()` - 测试并发查询

---

### Task 1.6: 集成测试 ⏱️ 1-2小时

**文件**: `tests/engine_integration.rs`

- [ ] 创建集成测试文件
- [ ] 测试完整工作流
  - [ ] `test_full_workflow()` - 创建、构建、查询、更新
  - [ ] `test_persistence()` - 保存、重启、加载
  - [ ] `test_concurrent_access()` - 多个客户端并发访问

- [ ] 性能基准测试
  - [ ] `bench_snapshot_performance()` - 快照性能
  - [ ] `bench_query_performance()` - 查询性能
  - [ ] `bench_rebuild_performance()` - 重建性能

**验证标准**:
```bash
cargo test --test engine_integration
cargo bench --bench engine_bench
```

---

## 🔄 阶段 2: LSP 迁移 (Day 4-5)

### Task 2.1: 更新 LspServer 结构 ⏱️ 1小时

**文件**: `src/lsp/mod.rs`

- [ ] 替换引擎字段
  ```rust
  // 旧代码
  // pub engine: Arc<RwLock<Option<Naviscope>>>,
  
  // 新代码
  pub engine: EngineHandle,
  ```

- [ ] 更新 `LspServer::new()`
  - [ ] 移除 `Arc::new(RwLock::new(None))`
  - [ ] 暂时使用空路径初始化（在 initialize 时更新）

- [ ] 更新 `initialize` 方法
  - [ ] 创建 `EngineHandle::new(root_path)`
  - [ ] 移除 `spawn_indexer` 调用
  - [ ] 直接在 EngineHandle 上调用 load + watch

**验证标准**:
```bash
cargo build --bin naviscope
# LSP server 应该能编译通过
```

---

### Task 2.2: 重构 indexer 模块 ⏱️ 1-2小时

**文件**: `src/lsp/indexer.rs`

- [ ] 简化 `spawn_indexer` 函数
  ```rust
  pub fn spawn_indexer(
      path: PathBuf,
      client: Client,
      engine: EngineHandle,
  ) {
      tokio::spawn(async move {
          // 加载现有索引
          if let Ok(true) = engine.load().await {
              client.log_message(INFO, "Index loaded").await;
          }
          
          // 重建索引
          if let Err(e) = engine.rebuild().await {
              client.log_message(ERROR, format!("Rebuild failed: {}", e)).await;
          }
          
          // 启动监听
          if let Err(e) = engine.watch().await {
              client.log_message(ERROR, format!("Watch failed: {}", e)).await;
          }
      });
  }
  ```

- [ ] 删除旧的索引构建逻辑（已移至 engine 层）

**验证标准**:
```bash
cargo test --lib lsp::indexer::tests
```

---

### Task 2.3: 更新 LSP 功能实现 ⏱️ 2-3小时

**文件**: `src/lsp/hover.rs`, `src/lsp/goto.rs`, 等

- [ ] 更新 `hover::hover()`
  ```rust
  // 旧代码
  // let lock = server.engine.read().await;
  // let navi = lock.as_ref()?;
  // let graph = navi.graph();
  
  // 新代码
  let graph = server.engine.graph().await;
  ```

- [ ] 更新所有 LSP 功能
  - [ ] `hover.rs` - hover 功能
  - [ ] `goto.rs` - 跳转功能
  - [ ] `highlight.rs` - 高亮功能
  - [ ] `symbols.rs` - 符号功能
  - [ ] `hierarchy.rs` - 层级功能

- [ ] 删除所有 `Option<Naviscope>` 的检查逻辑

**验证标准**:
```bash
cargo test --lib lsp
# 所有 LSP 测试应该通过
```

---

### Task 2.4: 更新 MCP HTTP Server ⏱️ 30分钟

**文件**: `src/mcp/http.rs`

- [ ] 更新 `spawn_http_server` 签名
  ```rust
  pub fn spawn_http_server(
      client: Client,
      engine: EngineHandle,  // 改为 EngineHandle
      root_path: PathBuf,
      // ...
  )
  ```

- [ ] 更新 LSP `initialize` 中的调用

**验证标准**:
```bash
cargo test --lib mcp::http
```

---

## 🔌 阶段 3: MCP 迁移 (Day 6)

### Task 3.1: 更新 McpServer 结构 ⏱️ 30分钟

**文件**: `src/mcp/mod.rs`

- [ ] 替换引擎字段
  ```rust
  pub struct McpServer {
      pub(crate) tool_router: ToolRouter<Self>,
      pub(crate) engine: EngineHandle,  // 改为 EngineHandle
  }
  ```

- [ ] 更新 `McpServer::new()`
  ```rust
  pub fn new(engine: EngineHandle) -> Self {
      Self {
          tool_router: Self::tool_router(),
          engine,
      }
  }
  ```

---

### Task 3.2: 简化查询执行 ⏱️ 1小时

**文件**: `src/mcp/mod.rs`

- [ ] 删除 `get_or_build_index` 方法

- [ ] 简化 `execute_query`
  ```rust
  pub(crate) async fn execute_query(
      &self,
      query: GraphQuery,
  ) -> Result<CallToolResult, McpError> {
      // 新代码：直接使用 handle.query()
      let result = self.engine
          .query(&query)
          .await
          .map_err(|e| McpError::new(...))?;
      
      let json_str = serde_json::to_string_pretty(&result)?;
      Ok(CallToolResult::success(vec![Content::text(json_str)]))
  }
  ```

- [ ] 更新所有 MCP 工具方法
  - [ ] `find()`
  - [ ] `ls()`
  - [ ] `cat()`
  - [ ] `deps()`

**验证标准**:
```bash
cargo test --lib mcp
```

---

### Task 3.3: 测试 MCP 功能 ⏱️ 1小时

- [ ] 运行 MCP stdio server 测试
  ```bash
  cargo run --bin naviscope -- mcp --path .
  ```

- [ ] 测试所有 MCP 工具
  - [ ] `get_guide` - 获取指南
  - [ ] `find` - 查找符号
  - [ ] `ls` - 列出子元素
  - [ ] `cat` - 查看详情
  - [ ] `deps` - 依赖分析

**验证标准**: 所有 MCP 工具正常工作

---

## 🐚 阶段 4: Shell 迁移 (Day 7)

### Task 4.1: 更新 ShellContext ⏱️ 1小时

**文件**: `src/cli/shell/context.rs`

- [ ] 替换引擎字段
  ```rust
  #[derive(Clone)]
  pub struct ShellContext {
      pub engine: EngineHandle,  // 改为 EngineHandle
      pub current_node: Arc<RwLock<Option<String>>>,
  }
  ```

- [ ] 更新 `ShellContext::new()`
  ```rust
  pub fn new(engine: EngineHandle, current_node: Arc<RwLock<Option<String>>>) -> Self {
      Self { engine, current_node }
  }
  ```

- [ ] 更新所有方法使用同步接口
  - [ ] `resolve_node()` - 使用 `engine.graph_blocking()`
  - [ ] `resolve_special_path()` - 使用 `engine.graph_blocking()`

---

### Task 4.2: 更新 Completer ⏱️ 30分钟

**文件**: `src/cli/shell/completer.rs`

- [ ] 更新 `complete` 方法
  ```rust
  fn complete(&mut self, line: &str, pos: usize) -> Vec<Suggestion> {
      // 新代码：获取快照（极快）
      let graph = self.context.engine.graph_blocking();
      
      // 所有计算都在锁外进行
      // ...
  }
  ```

---

### Task 4.3: 更新 ReplServer ⏱️ 1小时

**文件**: `src/cli/shell/mod.rs`

- [ ] 更新 `ReplServer` 结构
  ```rust
  pub struct ReplServer {
      context: ShellContext,
      project_path: PathBuf,
  }
  ```

- [ ] 更新 `ReplServer::new()`
  ```rust
  pub fn new(project_path: PathBuf) -> Self {
      let engine = EngineHandle::new(project_path.clone());
      let current_node = Arc::new(RwLock::new(None));
      let context = ShellContext::new(engine, current_node);
      
      Self { context, project_path }
  }
  ```

- [ ] 更新 `initialize_index()`
  - [ ] 使用 `engine.load()` 和 `engine.rebuild()`
  - [ ] 注意：需要创建 tokio runtime 来运行异步代码
    ```rust
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        self.context.engine.load().await?;
        self.context.engine.rebuild().await?;
        Ok::<_, Error>(())
    })?;
    ```

- [ ] 删除 `start_watcher()` 方法
  - [ ] 改为调用 `engine.watch()`

---

### Task 4.4: 更新 Shell Handlers ⏱️ 30分钟

**文件**: `src/cli/shell/handlers.rs`

- [ ] 更新所有命令处理器使用同步接口
  - [ ] 使用 `context.engine.graph_blocking()`
  - [ ] 使用 `context.engine.query_blocking()`

**验证标准**:
```bash
cargo run --bin naviscope -- shell
# Shell 应该能正常启动和使用
```

---

## 🧪 阶段 5: 测试与优化 (Day 8-9)

### Task 5.1: 端到端测试 ⏱️ 2小时

- [ ] 测试 LSP Server
  - [ ] 在 VSCode/Cursor 中测试所有功能
  - [ ] hover, goto, references, symbols, 等

- [ ] 测试 MCP Server
  - [ ] 通过 HTTP 和 stdio 测试
  - [ ] 所有工具调用

- [ ] 测试 Shell REPL
  - [ ] 所有命令
  - [ ] Tab 补全
  - [ ] 文件监听

---

### Task 5.2: 性能基准测试 ⏱️ 2小时

**文件**: `benches/engine_bench.rs`

- [ ] 创建性能基准测试
  ```rust
  use criterion::{black_box, criterion_group, criterion_main, Criterion};
  
  fn bench_snapshot(c: &mut Criterion) { ... }
  fn bench_query(c: &mut Criterion) { ... }
  fn bench_rebuild(c: &mut Criterion) { ... }
  ```

- [ ] 运行基准测试
  ```bash
  cargo bench
  ```

- [ ] 记录性能数据
  - [ ] 快照获取时间
  - [ ] 查询响应时间
  - [ ] 内存使用情况

---

### Task 5.3: 并发压力测试 ⏱️ 2小时

**文件**: `tests/stress_test.rs`

- [ ] 创建压力测试
  - [ ] 100 个并发读者 + 1 个写者
  - [ ] 长时间运行（10 分钟）
  - [ ] 监控内存泄漏

- [ ] 运行压力测试
  ```bash
  cargo test --test stress_test --release -- --nocapture
  ```

---

### Task 5.4: 内存优化 ⏱️ 2小时

- [ ] 使用 `valgrind` 或 `heaptrack` 检测内存泄漏
- [ ] 检查旧版本图是否及时释放
- [ ] 优化快照生命周期管理

---

## 🧹 阶段 6: 清理与发布 (Day 10)

### Task 6.1: 删除旧代码 ⏱️ 1小时

- [ ] 删除旧的 `Naviscope` 结构（如果完全迁移）
- [ ] 清理未使用的导入和依赖
- [ ] 运行 `cargo clippy` 修复警告
- [ ] 运行 `cargo fmt` 格式化代码

---

### Task 6.2: 更新文档 ⏱️ 2小时

- [ ] 更新 `README.md`
  - [ ] 添加新架构说明
  - [ ] 更新性能数据

- [ ] 更新 `DESIGN.md`
  - [ ] 添加 Engine 层架构图
  - [ ] 说明 MVCC 模式

- [ ] 更新 `CODING_STYLE.md`
  - [ ] 添加引擎使用规范

- [ ] 创建 `CHANGELOG.md` 条目
  - [ ] 列出所有重大变更
  - [ ] 性能改进数据

---

### Task 6.3: 发布准备 ⏱️ 1小时

- [ ] 运行完整测试套件
  ```bash
  cargo test --all
  cargo test --all --release
  cargo clippy --all
  ```

- [ ] 更新版本号（`Cargo.toml`）
  - [ ] 从 `0.x.y` → `0.x+1.0` (breaking change)

- [ ] 创建 Git tag
  ```bash
  git tag -a v0.x.0 -m "Unified engine refactor"
  ```

- [ ] 合并到 main 分支
  ```bash
  git checkout main
  git merge refactor/unified-engine
  git push origin main --tags
  ```

---

## 📊 进度跟踪

### 完成度统计

- **阶段 0**: 100% ✅ (4/4 任务)
- **阶段 1**: 0% ⬜ (0/6 任务)
- **阶段 2**: 0% ⬜ (0/4 任务)
- **阶段 3**: 0% ⬜ (0/3 任务)
- **阶段 4**: 0% ⬜ (0/4 任务)
- **阶段 5**: 0% ⬜ (0/4 任务)
- **阶段 6**: 0% ⬜ (0/3 任务)

**总体进度**: 4/28 任务完成 (14%)

---

## 🎯 关键里程碑

- [ ] **Milestone 1** (Day 3): 引擎核心完成，所有单元测试通过
- [ ] **Milestone 2** (Day 5): LSP 迁移完成，可以正常使用
- [ ] **Milestone 3** (Day 6): MCP 迁移完成
- [ ] **Milestone 4** (Day 7): Shell 迁移完成，所有客户端迁移完毕
- [ ] **Milestone 5** (Day 9): 性能测试通过，达到预期目标
- [ ] **Milestone 6** (Day 10): 代码清理完成，准备发布

---

## 🚨 风险与应对

### 风险 1: tokio runtime 在 Shell 中的集成

**问题**: Shell 是同步程序，需要 tokio runtime 来运行异步代码

**应对**:
- 在 Shell 启动时创建 runtime
- 使用 `Handle::current().block_on()` 转换异步调用

### 风险 2: 性能未达预期

**问题**: Arc clone 或其他开销可能超出预期

**应对**:
- 阶段 5 重点进行性能测试
- 如果不达标，考虑进一步优化（如使用 parking_lot）

### 风险 3: 内存泄漏

**问题**: MVCC 可能导致旧版本无法释放

**应对**:
- 限制快照生命周期
- 添加内存监控和告警

---

## 📝 日志模板

每天结束时填写：

### Day X 工作日志

**日期**: YYYY-MM-DD  
**工作时间**: X 小时  
**完成任务**:
- [ ] Task X.Y - 任务描述

**遇到的问题**:
- 问题描述
- 解决方案

**明天计划**:
- [ ] Task X.Y - 任务描述

**备注**:
- 其他想法或发现

---

## ✅ 验收标准

重构完成后，必须满足以下所有标准：

### 功能验收
- [ ] 所有现有功能正常工作（LSP/MCP/Shell）
- [ ] 无回归 bug
- [ ] 所有测试通过（单元测试 + 集成测试）

### 性能验收
- [ ] 快照获取 < 10μs (当前: ~50ms)
- [ ] 内存占用减少 > 80% (当前: 10 个查询 ~50MB)
- [ ] 索引重建期间查询不阻塞（响应时间 < 10ms）

### 代码质量验收
- [ ] 所有 clippy 警告修复
- [ ] 代码覆盖率 > 70%
- [ ] 文档完整（所有公共 API 有文档注释）

### 用户体验验收
- [ ] LSP 响应更快（用户可感知）
- [ ] MCP 查询无超时
- [ ] Shell 补全更流畅

---

## 🎉 完成标志

当所有以上任务完成，且验收标准全部满足时，重构项目宣告完成！

**预期成果**:
- ✅ 统一的引擎架构
- ✅ 性能提升 90%+
- ✅ 代码质量显著提升
- ✅ 更好的可维护性和扩展性
