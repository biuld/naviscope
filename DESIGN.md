# Naviscope 设计方案 (V2 - 图模型)

## 1. 概览与核心理念

**Naviscope** 是一个为大语言模型（LLM）设计的、**基于图的结构化代码查询引擎**。其核心是构建一个能够同时理解**源代码微观语义**（调用、继承）和**项目级宏观结构**（包、依赖、特性）的统一**代码知识图谱**。

它旨在超越传统的文本搜索和简单的符号列表，提供一种类似数据库查询的、深度结构化的代码探索能力。

**V1 目标**: 优先支持 **Java + Gradle** 项目。

**核心功能:**

1.  **代码知识图谱构建**: 解析 Gradle 构建脚本和 Java 源代码，生成一个包含代码实体及其复杂关系的图。
2.  **结构化图查询**: 提供一个强大的查询接口，允许对图进行遍历和查询。例如：
    *   “找到函数 A 的所有调用者。”
    *   “列出所有实现了接口 B 的类。”
    *   “找出所有使用了 `com.google.guava` 库的代码文件。”

**设计目标:**

*   **深度理解**: 不仅知道“有什么”，更知道“它们之间有什么关系”。
*   **LLM 友好**: API 接口清晰，输入为结构化查询，输出为结构化数据（JSON）。
*   **可扩展**: 语言中立的抽象层设计，便于未来支持其他语言（如 Python, Rust）。

---

## 2. 系统架构

### 2.1. 核心: 代码知识图谱 (The Code Knowledge Graph)

我们将使用 `petgraph::Graph` 作为核心数据结构。

*   **节点 (Nodes)**: 代表项目中的一个“实体”。我们使用枚举来区分不同类型的实体（Code 或 Build）。
    ```rust
    pub enum GraphNode {
        Code(CodeElement),
        Build(BuildElement),
    }
    ```
*   **边 (Edges)**: 代表实体之间的“关系”，这是整个设计的精髓。
    ```rust
    pub enum EdgeType {
        Contains,
        InheritsFrom,
        Implements,
        Calls,
        References,
        Instantiates,
        UsesDependency,
    }
    ```

### 2.2. 结构化代码模型 (Graph-Native Model)

为了实现 LLM 友好的深度查询，我们采用**图原生 (Graph-native)** 的模型设计。这意味着我们不再在 `Class` 结构体中嵌套 `Method` 列表，而是将它们都作为独立的节点放入图中，并通过 `Contains` 边来表示包含关系。

针对 V1 的 Java，主要的结构体定义如下：
*   `JavaClass`: 包含名称、完全限定名 (FQN)、修饰符等。
*   `JavaMethod`: 包含名称、FQN、返回类型、参数列表等。

这种设计的优势在于：
1.  **减少冗余**: 避免在多个地方存储相同的信息。
2.  **查询灵活**: 可以轻松查询“某个方法属于哪个类”或“某个类包含哪些方法”，只需改变图遍历的方向。
3.  **解耦**: 解析器只需提取节点和边，无需重构复杂的嵌套对象。

### 2.3. 索引流程 (三阶段 - 生产实现)

1.  **Phase 1: Scan & Parse (并行扫描与解析)**
    *   **目标**: 识别并解析项目中的所有相关文件（源码及构建配置）。
    *   **工具**: 使用 `ignore` crate 遵循 `.gitignore` 并行遍历，利用 `rayon` 线程池加速。
    *   **产出**: `Vec<ParsedFile>`，包含文件的 AST 数据、哈希值和元数据。

2.  **Phase 2: Resolve (语义关联与指令生成)**
    *   **目标**: 将散乱的解析结果转化为具有全局语义关系的图操作指令集。
    *   **过程**:
        *   **2.1 Build Resolution**: 首先处理构建文件（如 `build.gradle`），识别模块结构并生成 `ProjectContext`。
        *   **2.2 Language Resolution**: 随后并行处理所有源文件，根据 `ProjectContext` 进行 FQN 定位、归属推断及跨文件引用关联。
    *   **产出**: `Vec<GraphOp>`，一组描述如何构建图的幂等操作指令集。

3.  **Phase 3: Apply (串行合并)**
    *   **目标**: 将生成的指令应用到内存图谱中。
    *   **动作**: 串行遍历 `GraphOp` 指令，更新 `StableDiGraph`。由于 Phase 2 已完成所有重计算，此阶段速度极快且保证了线程安全。

---
## 3. 模块设计 (Module Design)

为了保持代码的组织性和可维护性，我们将 `naviscope` 库划分为以下几个核心模块。

```
naviscope/
├── src/
│   ├── lib.rs          # 库的根文件，定义模块结构
│   ├── error.rs        # 定义 NaviscopeError 和 Result 类型
│   ├── main.rs         # 命令行入口
│   │
│   ├── model/          # 数据模型
│   │   ├── mod.rs
│   │   ├── graph.rs    # 定义 GraphNode (JavaElement/GradleElement) 和 EdgeType
│   │   └── lang/       # 语言特定的详细模型
│   │       ├── mod.rs
│   │       ├── java.rs   # JavaClass, JavaMethod, JavaField 等
│   │       └── gradle.rs # GradleDependency
│   │
│   └── parser/         # 解析逻辑
│       ├── mod.rs
│       ├── utils.rs    # Query 加载和宏定义 (decl_indices!)
│       ├── java/       # Java 解析实现
│       │   ├── mod.rs  # 基于 Query 的解析逻辑
│       │   └── constants.rs
│       ├── gradle.rs   # Gradle 解析实现
│       └── queries/    # Tree-sitter 查询文件 (.scm)
│           ├── mod.rs
│           ├── java_definitions.scm
│           ├── java_definitions.rs
│           ├── gradle_definitions.scm
│           └── gradle_definitions.rs
│
├── build.rs            # 编译 tree-sitter C 源码
└── Cargo.toml
```

### 模块职责:

*   **`lib.rs`**: 作为项目的入口，负责组织其他模块，并向外部暴露最终的公共 API，如 `Naviscope` 主结构体及其方法。
*   **`error.rs`**: 集中定义项目中所有的自定义错误类型，使用 `thiserror` 来提供清晰、一致的错误处理体验。
*   **`model` 模块**:
    *   `model::graph`: 存放图的核心枚举 `GraphNode` 和 `EdgeType`。
    *   `model::lang`: 存放特定语言的 AST 抽象模型，如 `JavaClass`。
*   **`parser` 模块**:
    *   采用 **Tree-sitter Query** 驱动的解析模式。
    *   `parser::queries`: 存放 `.scm` 查询文件，通过 `decl_indices!` 宏在 Rust 中建立 capture 名称与索引的映射。
    *   `parser::java` / `parser::gradle`: 实现具体的解析逻辑，利用统一的查询捕获结果来提取实体和关系。
*   **`project` 模块**:
    *   负责文件系统的交互、增量更新和生命周期管理。
    *   `project::scanner`: 负责高效、并发地遍历文件系统。
    *   `project::watcher`: 负责监听文件变更事件。
    *   `project::source`: 定义 `SourceFile` 抽象。
    *   `project::resolver`: **核心语义组装层**，负责将解析结果转换为图操作。
        *   采用 **策略模式 (Strategy Pattern)**：基于 `(BuildTool, Language)` 组合动态选择解析策略。
        *   定义了两个核心枚举：
            *   `BuildTool`: Gradle, Maven, Cargo, NPM, Poetry, Bazel
            *   `Language`: Java, Kotlin, Rust, JavaScript, TypeScript, Python, Go
        *   `ResolverStrategy` trait: 统一的解析接口，所有策略实现此接口。
        *   `JavaGradleStrategy`: 实现 Java + Gradle 组合的解析逻辑。
        *   采用 **Map-Reduce** 模式：并行计算图操作指令，串行合并到图中。
*   **`build.rs`**: 在 `cargo build` 时运行的构建脚本。它的核心职责是找到 `tree-sitter` grammars 的源文件并使用 `cc` crate 将它们编译成静态库，以供我们的解析器使用。

这种模块化的结构使我们能够清晰地分离数据模型（`model`）、业务逻辑（`parser`）和系统交互（`project`），极大地提高了代码的可测试性和可扩展性。

---

## 3.1. Project 模块详细设计 (New)

为了支持**增量更新**和**长期运行服务**（如 LSP），我们需要引入更高级别的 `Project` 抽象，将文件系统与图数据解耦。

### 核心抽象: `SourceFile`

在图节点和物理文件之间引入中间层 `SourceFile`：

```rust
pub struct SourceFile {
    /// 文件的绝对路径（唯一标识）
    pub path: PathBuf,
    /// 内容哈希 (xxHash/Blake3)，用于快速检测内容变更
    pub content_hash: u64,
    /// 最后修改时间 (mtime)，辅助判断
    pub last_modified: SystemTime,
    /// 该文件贡献给图谱的所有节点索引 (用于删除时清理)
    pub owned_nodes: Vec<NodeIndex>,
    /// 语言类型
    pub language: LanguageType,
}
```

### 架构组件

1.  **Scanner (并行扫描)**
    *   使用 `ignore` crate（替代 `walkdir`）来尊重 `.gitignore` 规则。
    *   采用 `rayon` 进行**并行文件读取与解析**。
    *   首次索引时，多个 Parser 在线程池中并发工作，最后汇聚结果更新 Graph。

2.  **Watcher (并发监听)**
    *   使用 `notify` crate 监听项目根目录。
    *   在独立线程中接收文件事件（Create, Modify, Delete）。
    *   **Debounce**: 对短时间内的多次变更进行防抖处理。
    *   **处理流**: `Event -> Diff (Hash check) -> Index(WriteLock) -> Cleanup Old Nodes -> Parse New -> Merge New Nodes`.

3.  **Graph Storage Upgrade**
    *   **重要**: 为了支持节点的删除而不破坏其他索引，底层图结构应切换为 **`petgraph::stable_graph::StableGraph`**。`StableGraph` 保证删除节点后，其他节点的 `NodeIndex` 保持不变。

### 交互流程

*   **Initial Scan**: Scanner 并发遍历 -> 生成 `SourceFile` 列表 -> 并发 Parse -> 批量写入 Index。
*   **Incremental Update**: Watcher 捕获 `Modify` -> Check Hash -> Remove `owned_nodes` from Graph -> Parse -> Insert new nodes -> Update `SourceFile`.

---

## 4. V1 核心数据结构 (Java/Gradle)

```rust
// src/index.rs
pub struct NaviscopeIndex {
    pub version: u32,
    pub graph: StableDiGraph<GraphNode, EdgeType>,
    pub fqn_map: HashMap<String, NodeIndex>,
}

// src/model/graph.rs
pub enum GraphNode {
    Code(CodeElement),
    Build(BuildElement),
}

pub enum CodeElement {
    Java {
        element: JavaElement,
        file_path: Option<String>,
    },
}

pub enum BuildElement {
    Gradle {
        element: GradleElement,
        file_path: Option<String>,
    },
}

pub enum EdgeType {
    Contains,
    InheritsFrom,
    Implements,
    Calls,
    References,
    Instantiates,
    UsesDependency,
}

// src/model/lang/java.rs
pub enum JavaElement {
    Class(JavaClass),
    Interface(JavaInterface),
    Enum(JavaEnum),
    Annotation(JavaAnnotation),
    Method(JavaMethod),
    Field(JavaField),
}
```

---

## 4. API 设计 (Shell-like DSL)

为了对 LLM Agent 友好，查询接口采用了类 Shell 命令的设计模式，强调“动作 + 路径/符号”。

```rust
// API 输入 (src/query/mod.rs)
#[derive(Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum GraphQuery {
    /// 全局搜索符号 (Shell: grep)
    Grep {
        pattern: String,
        kind: Vec<String>,
        limit: usize,
    },
    /// 列出成员 (Shell: ls)
    Ls {
        fqn: String,
        kind: Vec<String>,
    },
    /// 查看详细信息 (Shell: inspect)
    Inspect {
        fqn: String,
    },
    /// 追踪入向关系：调用者、实现者等 (Shell: callers)
    Incoming {
        fqn: String,
        edge_type: Vec<EdgeType>,
    },
    /// 追踪出向关系：被调用者、依赖等 (Shell: callees)
    Outgoing {
        fqn: String,
        edge_type: Vec<EdgeType>,
    },
}
```

### 结果返回策略

*   **摘要模式 (`NodeSummary`)**: 对于 `grep`, `ls`, `incoming`, `outgoing` 命令，系统仅返回节点的摘要信息（FQN, Name, Kind），以节省 Token 并提高 Agent 处理效率。
*   **详情模式 (`GraphNode`)**: 仅当使用 `inspect` 命令时，系统才返回节点的完整序列化数据。

---

## 5. 依赖项 (Crates)

*   **`petgraph`**: **核心依赖**，用于图的创建和操作。
*   **`tree-sitter`**: 核心解析引擎。
*   **`tree-sitter-java`**, **`tree-sitter-groovy`**: V1 所需的语言 grammar。
*   **`serde`**, **`serde_json`**: 用于 API 的序列化和反序列化。
*   **`bincode`** / **`ciborium`**: 用于索引文件的二进制序列化。
*   **`walkdir`**: 用于遍历项目文件。
*   **`thiserror`**: 用于错误处理。
*   **`log`**, **`env_logger`**: 用于日志记录。
*   **(Optional) `rust-bert`**: 用于未来可能的语义+结构混合查询。

---

## 6. V1 工作流示例

1.  **[线下] 索引**:
    调用 `Naviscope::build_index("path/to/java-project", "naviscope.index")`。

2.  **[线上] LLM 发起查询**:
    LLM 想知道“哪个类实现了 `java.io.Serializable` 接口？”

3.  **[线上] 环境调用 API**:
    LLM 执行环境构造 JSON 查询:
    ```json
    {
      "query": "find_upstream",
      "node_id": "java.io.Serializable",
      "edge_filter": ["Implements"]
    }
    ```
    然后调用 `naviscope.execute_query(...)`。

4.  **[线上] Naviscope 处理**:
    Naviscope 在图中找到 `java.io.Serializable` 节点，然后查找所有指向它的、类型为 `Implements` 的**入边**，收集这些边的来源节点（即实现了该接口的类）。

5.  **[线上] 返回结果**:
    返回一个包含所有找到的 `JavaClass` 信息的 JSON 数组。

6.  **[线上] LLM 解析**:
    LLM 接收到 JSON，获得所有实现类的列表。

---

## 7. V1 实施局限性

*   **Gradle 解析**: V1 将只支持解析在 `dependencies` 块中以字符串字面量形式声明的依赖，无法解析来自变量或父级 `pom` 的版本。
*   **Java 类型推导**: V1 的方法调用关系建立将基于简单的名称匹配，暂不进行完整的本地变量类型推导。

---

## 8. Resolver 架构详解 (Map-Reduce Pattern)

### 8.1. 为什么需要 Resolver？

在解析代码文件和构建图谱之间，存在一个关键的"语义鸿沟"：
- **Scanner/Parser** 告诉我们文件中**有什么**（类、方法、依赖字符串）。
- **Graph** 需要知道这些东西**在项目拓扑中的位置**（属于哪个模块？哪个包？）。

**Resolver** 就是连接这两者的桥梁，负责：
1.  **物理归属推断** (Module Resolution)：通过文件路径向上查找 `build.gradle`，确定代码属于哪个构建模块。
2.  **逻辑命名空间提取** (Package Resolution)：从 AST 中提取包声明（如 `package com.example;`）。
3.  **层级关系构建**：建立 `Module -> Package -> Class -> Method` 的完整层级。

### 8.2. 策略模式设计 (Strategy Pattern)

**核心思想**：不同的 `(BuildTool, Language)` 组合需要不同的解析策略。为了最大化复用，我们将解析逻辑拆分为两个维度的策略：

#### 1. BuildResolver (构建工具维度)
负责解析项目结构、模块依赖及建立 `ProjectContext`。
- `GradleResolver`: 处理 `build.gradle` / `settings.gradle`。
- `CargoResolver`: (待实现) 处理 `Cargo.toml`。

#### 2. LangResolver (编程语言维度)
负责解析具体语言的符号定义、引用及局部关系。
- `JavaResolver`: 使用 Tree-sitter Java 语法提取类、方法、字段。
- `RustResolver`: (待实现) 处理 Rust 源码。

#### 二阶段编排

```rust
pub struct Resolver {
    build_strategies: HashMap<BuildTool, Box<dyn BuildResolver>>,
    lang_strategies: HashMap<Language, Box<dyn LangResolver>>,
}
```

### 8.3. 三阶段处理流程 (Phase 1-3)

```
Phase 1: Scan & Parse (并行)
  - 使用 Scanner 遍历文件系统。
  - 并行调用各语言 Parser 提取 AST 数据。

Phase 2: Resolve (逻辑关联)
  - 2.1 Build Resolution: 调用 BuildResolver 建立模块树。
  - 2.2 Lang Resolution: 并行调用 LangResolver，根据 Context 生成 GraphOp。

Phase 3: Apply (串行合并)
  - 串行执行 GraphOp 指令更新内存图谱。
```

### 8.4. 设计优势

1.  **性能卓越**：最耗时的路径分析、字符串处理都在并行阶段完成。
2.  **多语言通用**：每种语言只需实现自己的 `ResolverStrategy`（如 `RustCargoStrategy`、`PythonPoetryStrategy`）。
3.  **易于测试**：Resolver 是纯函数（输入 ParsedFile，输出 GraphOp），无副作用。
4.  **可扩展**：未来可以添加更多类型的 GraphOp（如 `UpdateNode`、`RemoveNode`）。

### 8.5. 模块与包的关联策略

**核心原则**：
- **Module (物理模块)**：由**文件系统路径**决定（向上查找锚点文件，如 `build.gradle`、`Cargo.toml`、`package.json`）。
- **Package (逻辑命名空间)**：由**源代码内容**决定（AST 中的 `package` 声明）。

**图谱层级示例**：
```
[Module: :app] --(Contains)--> [Package: :app::com.example] --(Contains)--> [Class: UserService]
     |
     +-- (UsesDependency) --> [Dependency: guava:31.1-jre]
```

**多语言通用性**：

| 语言 | Module 锚点文件 | Package 来源 |
|------|----------------|--------------|
| Java | `build.gradle` / `pom.xml` | AST: `package com.foo;` |
| Rust | `Cargo.toml` | AST: `mod` 声明 + 文件结构 |
| Go | `go.mod` | AST: `package main` |
| Python | `pyproject.toml` / `setup.py` | 文件系统: `__init__.py` |
| JavaScript | `package.json` | 文件系统: 目录结构 |

### 8.6. 实现细节

**JavaGradleStrategy 关键逻辑**：
1. 从文件路径向上冒泡查找 `build.gradle`，确定所属模块（如 `:app`）。
2. 从 AST 提取 `package` 声明（如 `com.example`）。
3. 生成模块绑定的包节点 ID：`:app::com.example`。
4. 生成图操作：
   - `AddNode(module:":app")`
   - `AddNode(package:":app::com.example")`
   - `AddEdge(module -> package, Contains)`
   - `AddEdge(package -> class, Contains)`

**未来扩展示例**：

添加 `RustCargoStrategy` 支持：
```rust
strategies.insert(
    (BuildTool::Cargo, Language::Rust),
    Box::new(RustCargoStrategy::new())
);
```

这种设计确保了：
- **避免 Split Package 问题**：同名包在不同模块中是独立节点。
- **支持跨语言查询**：Module 和 Package 的概念在所有语言中统一。
- **高性能**：并行计算 + 串行合并，充分利用多核 CPU。


