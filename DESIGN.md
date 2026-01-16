# Naviscope 设计方案 (V2 - 图模型)

## 1. 概览与核心理念

**Naviscope** 是一个为大型语言模型（LLM）设计的、**基于图的结构化代码查询引擎**。其核心是构建一个能够同时理解**源代码微观语义**（调用、继承）和**项目级宏观结构**（包、依赖、特性）的统一**代码知识图谱**。

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

*   **节点 (Nodes)**: 代表项目中的一个“实体”。我们使用枚举来区分不同类型的实体。
    ```rust
    pub enum GraphNode {
        /// 来自源代码的元素 (e.g., class, method, field)
        Code(Box<dyn CodeElement>),
        /// 来自构建配置文件的元素 (e.g., dependency, package)
        Build(BuildElement),
    }
    ```
*   **边 (Edges)**: 代表实体之间的“关系”，这是整个设计的精髓。
    ```rust
    pub enum EdgeType {
        // 结构关系
        Contains,      // File contains Class, Class contains Method
        // 继承/实现关系
        InheritsFrom,  // Class extends Superclass
        Implements,    // Class implements Interface
        // 调用/引用关系
        Calls,         // Method A calls Method B
        References,    // Method references Field
        Instantiates,  // Method instantiates Class
        // 依赖关系
        UsesDependency, // File/Package uses a library
    }
    ```

### 2.2. 语言中立的抽象层 (A "PSI-like" Abstract Layer)

为了实现可扩展性，我们使用 Rust 的 `trait` 系统来定义一套通用的代码元素接口，类似于 IntelliJ 的 PSI。

```rust
// 伪代码示例
// 所有代码元素的基石
pub trait CodeElement { /* ... */ }
// 可被调用的实体，如函数、方法
pub trait Callable: CodeElement { /* ... */ }
// 包含成员的容器，如类、接口
pub trait Container: CodeElement { /* ... */ }
```
针对 V1 的 Java，我们将创建具体的 `struct` 来实现这些 `trait`：
*   `JavaClass`: `impl Container, CodeElement`
*   `JavaMethod`: `impl Callable, CodeElement`

### 2.3. 索引流程 (三阶段)

1.  **Pass 0: 构建配置解析 (Build Configuration Pass)**
    *   **目标**: 解析 `build.gradle` 和 `settings.gradle`。
    *   **动作**: 使用 `tree-sitter-groovy` 提取项目（package）、依赖（dependency）等信息，并作为 `GraphNode::Build` 节点添加到图中，形成项目的宏观骨架。

2.  **Pass 1: 符号定义收集 (Definition Pass)**
    *   **目标**: 解析所有 `.java` 文件，识别符号定义。
    *   **动作**: 提取 `class`, `interface`, `method` 等定义，创建对应的 `JavaClass`, `JavaMethod` 等结构体，作为 `GraphNode::Code` 节点添加到图中。同时，建立一个从完全限定名到图中 `NodeIndex` 的映射，供下一阶段使用。

3.  **Pass 2: 关系建立 (Relationship Pass)**
    *   **目标**: 再次遍历 `.java` 文件，建立节点之间的边。
    *   **动作**:
        *   解析 `extends`/`implements`，添加 `InheritsFrom`/`Implements` 边。
        *   解析方法调用、字段引用，添加 `Calls`/`References` 边。
        *   解析 `import` 语句，将其与 Pass 0 中创建的 `Dependency` 节点关联，添加 `UsesDependency` 边。

---
## 3. 模块设计 (Module Design)

为了保持代码的组织性和可维护性，我们将 `naviscope` 库划分为以下几个核心模块。

```
naviscope/
├── src/
│   ├── lib.rs          # 库的根文件，定义公共 API 和模块结构
│   ├── error.rs        # 定义 NaviscopeError 枚举和错误处理类型
│   │
│   ├── model/          # 核心数据模型模块
│   │   ├── mod.rs      # 声明 model 子模块
│   │   ├── graph.rs    # 定义 GraphNode, EdgeType 和其他图相关结构
│   │   └── psi.rs      # 定义语言中立的 PSI traits 和具体的 Java 元素 struct
│   │
│   └── parser/         # 解析逻辑模块
│       ├── mod.rs      # 声明 parser 子模块
│       ├── core.rs     # 定义通用的解析器 trait (e.g., LanguageParser)
│       ├── java.rs     # Java 语言解析器的具体实现
│       └── gradle.rs   # Gradle (Groovy) 构建文件的解析器实现
│
├── build.rs            # 构建脚本，用于编译 tree-sitter grammars
└── Cargo.toml
```

### 模块职责:

*   **`lib.rs`**: 作为项目的入口，负责组织其他模块，并向外部暴露最终的公共 API，如 `Naviscope` 主结构体及其方法。
*   **`error.rs`**: 集中定义项目中所有的自定义错误类型，使用 `thiserror` 来提供清晰、一致的错误处理体验。
*   **`model` 模块**:
    *   `model::graph`: 存放代码知识图谱的核心构建块，即 `GraphNode` 和 `EdgeType` 枚举。
    *   `model::psi`: 存放类似 IntelliJ PSI 的抽象层。`psi` (Program Structure Interface) 包含了语言无关的 `trait`（如 `CodeElement`, `Container`）和特定于语言的结构体（如 `JavaClass`, `JavaMethod`）。
*   **`parser` 模块**:
    *   封装所有与 `tree-sitter` 相关的解析逻辑。
    *   `parser::core`: 可以定义一个通用的 `LanguageParser` trait，规范化不同解析器的接口。
    *   `parser::java` / `parser::gradle`: 分别实现对 Java 源代码和 Gradle 构建脚本的解析逻辑，它们将负责从语法树中提取信息并填充到我们的图模型中。
*   **`build.rs`**: 在 `cargo build` 时运行的构建脚本。它的核心职责是找到 `tree-sitter` grammars 的源文件并使用 `cc` crate 将它们编译成静态库，以供我们的解析器使用。

这种模块化的结构使我们能够清晰地分离数据模型（`model`）和业务逻辑（`parser`），极大地提高了代码的可测试性和可扩展性。

---

## 4. V1 核心数据结构 (Java/Gradle)

```rust
// main.rs or lib.rs
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use petgraph::graph::DiGraph;

// --- 抽象层 Traits ---
// ... as described in 2.2 ...

// --- 图的节点和边 ---
#[derive(Serialize, Deserialize)]
pub enum GraphNode {
    Code(JavaElement), // V1 Specific
    Build(BuildElement),
}

#[derive(Serialize, Deserialize)]
pub enum EdgeType {
    Contains,
    Implements,
    InheritsFrom,
    Calls,
    References,
    UsesDependency,
}

// --- Java 具体实现 ---
#[derive(Serialize, Deserialize)]
pub enum JavaElement {
    Class(JavaClass),
    Method(JavaMethod),
    // ...
}
#[derive(Serialize, Deserialize)] pub struct JavaClass { /* ... */ }
#[derive(Serialize, Deserialize)] pub struct JavaMethod { /* ... */ }


// --- Gradle 具体实现 ---
#[derive(Serialize, Deserialize)]
pub enum BuildElement {
    Package(GradlePackage),
    Dependency(GradleDependency),
}
#[derive(Serialize, Deserialize)] pub struct GradlePackage { pub name: String, }
#[derive(Serialize, Deserialize)] pub struct GradleDependency { pub group: String, pub name: String, pub version: String }

// --- 主索引结构 ---
pub struct NaviscopeIndex {
    pub graph: DiGraph<GraphNode, EdgeType>,
    // Map from FQN (String) to NodeIndex for fast lookups
    pub fqn_map: std::collections::HashMap<String, petgraph::graph::NodeIndex>,
}
```

---

## 4. API 设计

查询将通过一个统一的、可扩展的 `GraphQuery` 枚举来执行。

```rust
// API 输入
#[derive(Deserialize)]
pub enum GraphQuery {
    FindNode {
        name: String,
        kind: Option<String>,
    },
    FindUpstream { // 查找调用者、实现者等
        node_id: String, // FQN of the starting node
        edge_filter: Vec<EdgeType>,
    },
    FindDownstream { // 查找被调用者、字段等
        node_id: String,
        edge_filter: Vec<EdgeType>,
    },
    // ... more complex queries in the future
}

// 主结构体
pub struct Naviscope {
    index: NaviscopeIndex,
}

impl Naviscope {
    // ... load_from_index, build_index ...

    /// 执行结构化的图查询
    pub fn execute_query(&self, query: &GraphQuery) -> Result<serde_json::Value, NaviscopeError> {
        // ... implementation of graph traversal ...
    }
}
```

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
