# Type System 架构深度剥离与接入方案 (v2)

## 1. 核心哲学：语义解耦

我们不应将 Type System 视为一个额外的“功能模块”，而应将其视为**插件的语义大脑**。
*   `LspService`: 负责**语法与局部上下文**（提取符号、查找词法引用）。
*   `SemanticResolver`: 负责**图查询与全局映射**（查找定义、实现、解析 FQN）。
*   `TypeSystem` (新): 负责**逻辑判断与关系推理**（子类型、重写、匹配验证）。

## 2. 插件层 (naviscope_plugin) 调整

在 `naviscope_plugin` 中确立 `TypeSystem` 的地位，并让其他 Service 能够引用它。

### 2.1 定义独立 Trait
```rust
// crates/plugin/src/type_system.rs

pub trait TypeSystem: Send + Sync {
    /// 核心判断逻辑：在给定的图上下文中，candidate 是否是 target 的一个有效语义引用？
    /// 这封装了继承、匹配、重写等所有复杂逻辑。
    fn is_reference_to(
        &self,
        graph: &dyn CodeGraph,
        candidate: &SymbolResolution,
        target: &SymbolResolution,
    ) -> bool;

    /// 获取类型层级关系
    fn is_subtype(&self, graph: &dyn CodeGraph, sub: &str, sup: &str) -> bool;
}
```

### 2.2 统一生命周期
语言插件（如 `JavaPlugin`）不再零散地创建服务，而是统一初始化：
1.  初始化 `LanguageTypeSystem`。
2.  将该 `TypeSystem` 分别注入给 `LspService` 和 `SemanticResolver`。

```rust
// 插件接口更新
pub trait LanguagePlugin: Send + Sync {
    fn type_system(&self) -> Arc<dyn TypeSystem>;
    fn lsp_service(&self) -> Arc<dyn LspService>; // 内部持有 type_system
    fn semantic_resolver(&self) -> Arc<dyn SemanticResolver>; // 内部持有 type_system
}
```

## 3. Java 插件的实现微调 (lang-java)

通过“依赖注入”消除重复逻辑：
*   **JavaTypeSystem**: 包装 `crates/lang-java/src/inference` 中的核心推理引擎。
*   **JavaLspService**: 在执行 `find_occurrences` 时，利用注入的 `TypeSystem` 进行局部验证。
*   **JavaResolver**: 在执行 `resolve_at` 时，利用注入的 `TypeSystem` 关联图节点。

## 4. 核心引擎 (crates/core) 的极简调用

`DiscoveryEngine` 的 `scan_file` 逻辑将变得极其纯粹，它只负责流程编排，不持有任何判断逻辑：

```rust
// crates/core/src/features/discovery.rs

pub fn scan_file(
    &self,
    lsp_service: &dyn LspService,
    type_system: &dyn TypeSystem, // 新增参数
    resolver: &dyn SemanticResolver,
    source: &str,
    target_resolution: &SymbolResolution,
) -> Vec<Location> {
    // 1. 委托 LspService 进行快速语法扫描
    let candidates = lsp_service.find_occurrences(source, &tree, target_resolution);

    // 2. 委托 Resolver 进行实时点位解析
    for range in candidates {
        if let Some(resolved) = resolver.resolve_at(...) {
            // 3. 委托 TypeSystem 进行最终的语义身份验证
            if type_system.is_reference_to(self.index, &resolved, target_resolution) {
                valid_locations.push(...);
            }
        }
    }
}
```

## 5. 方案优势

1.  **实现不重复**：复杂的 Java 类型逻辑（如 Bridge Method, Generics）只在 `JavaTypeSystem` 中写一次。
2.  **职责明确**：`LspService` 不再需要理解什么是“子类方法”，它只管找“在这个作用域内叫这个名字的符号”。
3.  **可测试性**：可以给 `scan_file` 注入一个 `MockTypeSystem` 来测试核心流程。

## 6. 改造清单

- [ ] **naviscope-plugin**: 新增 `TypeSystem` trait，并将其加入 `LanguagePlugin` 初始化流。
- [ ] **lang-java**: 将现有的 `inference` 代码包装为 `TypeSystem` 实现。
- [ ] **lang-java**: 重构 `JavaLspService` 和 `JavaResolver`，使其通过构造函数接收 `TypeSystem`。
- [ ] **core**: 在获取服务时，同步获取 `TypeSystem` 并传递给 `DiscoveryEngine`。
