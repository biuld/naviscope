// ============================================================================
// Naviscope Engine 重构原型
//
// 这是一个简化的原型，展示核心设计概念：
// 1. Arc 包装的不可变 CodeGraph（廉价克隆）
// 2. MVCC 模式的 NaviscopeEngine（非阻塞读取）
// 3. 统一的 EngineHandle（支持异步/同步）
// ============================================================================

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

// ============================================================================
// 1. 不可变的图数据（使用 Arc 共享）
// ============================================================================

/// 不可变的代码图
///
/// 特点：
/// - 所有数据包装在 Arc 中
/// - clone() 只增加引用计数，不复制数据（O(1) 复杂度）
/// - 线程安全，可以在多个线程/任务间共享
#[derive(Clone)]
pub struct CodeGraph {
    inner: Arc<CodeGraphInner>,
}

struct CodeGraphInner {
    version: u32,
    // 简化：这里只保留 fqn_map 作为示例
    // 实际应该包含 topology, name_map, file_map 等
    fqn_map: HashMap<String, NodeData>,
}

#[derive(Clone)]
struct NodeData {
    fqn: String,
    name: String,
    kind: String,
}

impl CodeGraph {
    /// 创建空图
    pub fn empty() -> Self {
        Self {
            inner: Arc::new(CodeGraphInner {
                version: 1,
                fqn_map: HashMap::new(),
            }),
        }
    }

    /// 创建构建器（用于修改）
    ///
    /// 这会进行一次深拷贝，但只在索引构建/更新时调用
    /// 查询时不会调用此方法
    pub fn to_builder(&self) -> CodeGraphBuilder {
        CodeGraphBuilder {
            version: self.inner.version,
            fqn_map: self.inner.fqn_map.clone(), // 深拷贝
        }
    }

    /// 查找节点（只读访问）
    pub fn find_node(&self, fqn: &str) -> Option<&NodeData> {
        self.inner.fqn_map.get(fqn)
    }

    /// 获取所有 FQN
    pub fn all_fqns(&self) -> Vec<String> {
        self.inner.fqn_map.keys().cloned().collect()
    }

    /// 获取节点数量
    pub fn node_count(&self) -> usize {
        self.inner.fqn_map.len()
    }
}

/// 图的构建器（可变）
///
/// 用于在索引构建/更新时修改图结构
pub struct CodeGraphBuilder {
    version: u32,
    fqn_map: HashMap<String, NodeData>,
}

impl CodeGraphBuilder {
    pub fn new() -> Self {
        Self {
            version: 1,
            fqn_map: HashMap::new(),
        }
    }

    /// 添加节点
    pub fn add_node(&mut self, fqn: String, name: String, kind: String) {
        self.fqn_map
            .insert(fqn.clone(), NodeData { fqn, name, kind });
    }

    /// 删除节点
    pub fn remove_node(&mut self, fqn: &str) {
        self.fqn_map.remove(fqn);
    }

    /// 完成构建，返回不可变的 CodeGraph
    pub fn build(self) -> CodeGraph {
        CodeGraph {
            inner: Arc::new(CodeGraphInner {
                version: self.version,
                fqn_map: self.fqn_map,
            }),
        }
    }
}

// ============================================================================
// 2. 引擎层（管理版本和并发）
// ============================================================================

/// 索引引擎
///
/// 核心设计：
/// - 使用 Arc<RwLock<Arc<CodeGraph>>> 管理当前版本
/// - 读者获取 Arc<CodeGraph> 快照（廉价）
/// - 写者创建新版本并原子替换
pub struct NaviscopeEngine {
    /// 当前版本的图（双层 Arc）
    ///
    /// 外层 Arc: 允许多个 EngineHandle 共享同一个引擎
    /// RwLock: 保护版本切换操作
    /// 内层 Arc: 允许多个读者共享同一个图
    current: Arc<RwLock<Arc<CodeGraph>>>,

    project_root: PathBuf,
}

impl NaviscopeEngine {
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            current: Arc::new(RwLock::new(Arc::new(CodeGraph::empty()))),
            project_root,
        }
    }

    /// 获取当前图的快照
    ///
    /// 复杂度：O(1) - 仅增加引用计数
    /// 阻塞：极短（仅读锁获取时间，微秒级）
    pub async fn snapshot(&self) -> CodeGraph {
        let lock = self.current.read().await;
        CodeGraph::clone(&*lock) // Arc::clone，不复制数据
    }

    /// 重建索引（后台执行）
    ///
    /// 流程：
    /// 1. 在 blocking pool 中构建新图
    /// 2. 获取写锁（极短时间）
    /// 3. 原子替换当前版本
    /// 4. 旧版本的读者不受影响（继续使用旧快照）
    pub async fn rebuild(&self) -> anyhow::Result<()> {
        let project_root = self.project_root.clone();

        // 1. 在 blocking pool 中构建新图（不持有任何锁）
        let new_graph = tokio::task::spawn_blocking(move || {
            // 模拟索引构建
            let mut builder = CodeGraphBuilder::new();

            // 这里应该调用 Scanner::scan_and_parse()
            // 为了演示，我们手动添加一些节点
            builder.add_node(
                "com.example.Main".to_string(),
                "Main".to_string(),
                "Class".to_string(),
            );
            builder.add_node(
                "com.example.Utils".to_string(),
                "Utils".to_string(),
                "Class".to_string(),
            );

            builder.build()
        })
        .await
        .map_err(|e| anyhow::anyhow!("Task join error: {}", e))?;

        // 2. 原子更新（写锁只持有数微秒）
        {
            let mut lock = self.current.write().await;
            *lock = Arc::new(new_graph);
        }

        println!("[Engine] Index rebuilt for {:?}", self.project_root);

        Ok(())
    }

    /// 增量更新
    pub async fn update_file(&self, file: PathBuf) -> anyhow::Result<()> {
        // 1. 获取当前图的构建器
        let current = self.snapshot().await;
        let mut builder = current.to_builder();

        // 2. 在 blocking pool 中解析文件并更新
        let new_graph = tokio::task::spawn_blocking(move || {
            // 模拟文件解析和更新
            builder.add_node(
                format!("file::{}", file.display()),
                file.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string(),
                "File".to_string(),
            );

            builder.build()
        })
        .await
        .map_err(|e| anyhow::anyhow!("Task join error: {}", e))?;

        // 3. 原子更新
        {
            let mut lock = self.current.write().await;
            *lock = Arc::new(new_graph);
        }

        println!("[Engine] File {:?} updated", file);

        Ok(())
    }
}

// ============================================================================
// 3. 客户端句柄（统一接口）
// ============================================================================

/// 引擎句柄
///
/// 提供统一的访问接口，隐藏底层锁管理细节
#[derive(Clone)]
pub struct EngineHandle {
    engine: Arc<NaviscopeEngine>,
}

impl EngineHandle {
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            engine: Arc::new(NaviscopeEngine::new(project_root)),
        }
    }

    // ---- 异步接口（用于 LSP/MCP）----

    /// 获取图快照（异步）
    pub async fn graph(&self) -> CodeGraph {
        self.engine.snapshot().await
    }

    /// 执行查询（异步）
    pub async fn query(&self, pattern: &str) -> Vec<String> {
        let graph = self.graph().await;

        // 在 blocking pool 执行查询（避免阻塞 async runtime）
        let pattern = pattern.to_string();
        tokio::task::spawn_blocking(move || {
            graph
                .all_fqns()
                .into_iter()
                .filter(|fqn| fqn.contains(&pattern))
                .collect()
        })
        .await
        .unwrap_or_default()
    }

    /// 重建索引（异步）
    pub async fn rebuild(&self) -> anyhow::Result<()> {
        self.engine.rebuild().await
    }

    /// 更新文件（异步）
    pub async fn update_file(&self, file: PathBuf) -> anyhow::Result<()> {
        self.engine.update_file(file).await
    }

    // ---- 同步接口（用于 Shell）----

    /// 获取图快照（同步）
    ///
    /// 注意：需要在 tokio runtime 中调用
    pub fn graph_blocking(&self) -> CodeGraph {
        tokio::runtime::Handle::current().block_on(self.engine.snapshot())
    }

    /// 执行查询（同步）
    pub fn query_blocking(&self, pattern: &str) -> Vec<String> {
        let rt = tokio::runtime::Handle::current();
        rt.block_on(self.query(pattern))
    }
}

// ============================================================================
// 4. 使用示例
// ============================================================================

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("=== Naviscope Engine 原型演示 ===\n");

    // 创建引擎句柄
    let engine = EngineHandle::new(PathBuf::from("."));

    // ---- 场景1：初始化索引 ----
    println!("场景1：初始化索引");
    engine.rebuild().await?;

    let graph = engine.graph().await;
    println!("  节点数: {}", graph.node_count());
    println!("  所有 FQN: {:?}\n", graph.all_fqns());

    // ---- 场景2：并发查询（不阻塞）----
    println!("场景2：启动 10 个并发查询任务");

    let mut query_tasks = vec![];
    for i in 0..10 {
        let e = engine.clone();
        let task = tokio::spawn(async move {
            for j in 0..5 {
                let results = e.query("example").await;
                println!("  [查询 {}:{}] 找到 {} 个结果", i, j, results.len());
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }
        });
        query_tasks.push(task);
    }

    // ---- 场景3：同时进行索引更新（不阻塞查询）----
    println!("\n场景3：在查询进行时更新索引");

    let update_task = {
        let e = engine.clone();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
            println!("  [更新] 开始重建索引...");
            e.rebuild().await.unwrap();
            println!("  [更新] 索引重建完成");

            tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
            println!("  [更新] 更新文件 test.rs...");
            e.update_file(PathBuf::from("test.rs")).await.unwrap();
            println!("  [更新] 文件更新完成");
        })
    };

    // 等待所有任务完成
    for task in query_tasks {
        task.await?;
    }
    update_task.await?;

    // ---- 场景4：验证最终状态 ----
    println!("\n场景4：验证最终状态");
    let final_graph = engine.graph().await;
    println!("  最终节点数: {}", final_graph.node_count());
    println!("  所有 FQN: {:?}", final_graph.all_fqns());

    // ---- 场景5：同步接口演示（Shell 场景）----
    println!("\n场景5：同步接口演示（模拟 Shell）");

    // 模拟在 Shell 中使用同步接口
    let shell_result = {
        let graph = engine.graph_blocking();
        println!("  [Shell] 当前节点数: {}", graph.node_count());

        let results = engine.query_blocking("test");
        println!("  [Shell] 查询 'test' 找到: {:?}", results);

        results
    };

    println!("\n=== 演示完成 ===");

    Ok(())
}

// ============================================================================
// 5. 性能测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_snapshot_is_cheap() {
        let engine = EngineHandle::new(PathBuf::from("."));
        engine.rebuild().await.unwrap();

        // 测试快照获取的性能
        let start = std::time::Instant::now();
        for _ in 0..10000 {
            let _graph = engine.graph().await;
            // 故意不使用 _graph，让它立即 drop
        }
        let elapsed = start.elapsed();

        println!("10000 次快照获取耗时: {:?}", elapsed);
        // 预期：应该在毫秒级（Arc clone 只是增加引用计数）
        assert!(elapsed.as_millis() < 100, "Snapshot should be cheap");
    }

    #[tokio::test]
    async fn test_concurrent_read_write() {
        let engine = EngineHandle::new(PathBuf::from("."));

        // 启动多个读者
        let mut readers = vec![];
        for _ in 0..100 {
            let e = engine.clone();
            readers.push(tokio::spawn(async move {
                for _ in 0..10 {
                    let graph = e.graph().await;
                    assert!(graph.node_count() >= 0);
                }
            }));
        }

        // 同时进行写操作
        let writer = {
            let e = engine.clone();
            tokio::spawn(async move {
                for i in 0..5 {
                    e.rebuild().await.unwrap();
                    println!("Rebuild {}/5 完成", i + 1);
                    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                }
            })
        };

        // 等待所有任务完成
        for r in readers {
            r.await.unwrap();
        }
        writer.await.unwrap();

        println!("并发测试通过：100 个读者 + 5 次重建");
    }
}
