# Naviscope

**Unified Code Knowledge Graph Engine for AI Agents & Developers**

Naviscope bridges the gap between AI and IDEs. It builds a comprehensive, graph-based representation of your codebase (connecting micro-level semantics like type relationships and inheritance with macro-level structures) that powers both **LLM agents** (via MCP) and **code editors** (via LSP).

Unlike traditional tools that maintain separate indexes for different purposes, Naviscope provides a **single, unified knowledge graph**, ensuring that what AI agents see is exactly what developers navigate.

## 💡 Why Naviscope?

| Feature | Traditional Tools | Naviscope |
| :--- | :--- | :--- |
| **Context** | Text-based (regex/grep) | **Graph-based** (structural/semantic) |
| **Performance** | High latency (JVM based) | **Instant** (Rust native, Zero-JVM overhead) |
| **Consistency** | Fragmented (Agent vs IDE) | **Unified** (Same graph for both) |
| **Resilience** | Blocks on errors/missing deps | **Robust** (Works with partial/broken code) |

## 🌟 Capabilities

### 🤖 For AI Agents (MCP Support)
Naviscope implements the [Model Context Protocol](https://modelcontextprotocol.io/), giving LLMs "X-ray vision" into your code structure.

- **`ls`**: Hierarchical exploration of packages, modules, and fields.
- **`find`**: Precise symbol search (find "Class definitions", not just string matches).
- **`cat`**: Retrieve definition, source code, and metadata for any symbol.
- **`deps`**: Analyze incoming/outgoing dependencies and relationships (inheritance, type usage, etc.).

### 👨‍💻 For Developers (LSP Support)
A lightweight, lightning-fast alternative to standard language servers (like JDTLS).

- **Navigation**: Go to Definition, Find References, Go to Implementation.
- **Understanding**: Hover documentation, Document Highlights.
- **Hierarchy**: Call Hierarchy, Type Hierarchy.
- **Speed**: Works immediately on large projects without long indexing pauses.

## 🏗️ Architecture

```mermaid
graph TD
    %% Styles
    classDef interface fill:#e3f2fd,stroke:#1565c0,stroke-width:2px,rx:5,ry:5
    classDef runtime fill:#f3e5f5,stroke:#7b1fa2,stroke-width:2px,rx:5,ry:5
    classDef language fill:#e8f5e9,stroke:#2e7d32,stroke-width:2px,rx:5,ry:5
    classDef core fill:#fff3e0,stroke:#ef6c00,stroke-width:2px,rx:5,ry:5
    classDef api fill:#fafafa,stroke:#616161,stroke-width:2px,rx:5,ry:5

    subgraph Interfaces [Interface Layer]
        CLI["naviscope-cli<br/>(Shell & Main)"]:::interface
        LSP["naviscope-lsp<br/>(LSP Server)"]:::interface
        MCP["naviscope-mcp<br/>(MCP Server)"]:::interface
    end

    subgraph Orchestration [Runtime Layer]
        Runtime["naviscope-runtime<br/>(Engine Orchestrator)"]:::runtime
    end

    subgraph Strategies [Language Layer]
        Java["naviscope-java<br/>(Java Analysis)"]:::language
        Gradle["naviscope-gradle<br/>(Gradle Analysis)"]:::language
    end

    subgraph Engine [Core Layer]
        Core["naviscope-core<br/>(Graph, Index & IO)"]:::core
    end

    subgraph Foundation [API Layer]
        API["naviscope-api<br/>(Common Traits & Models)"]:::api
    end

    %% Crate Dependencies
    CLI --> LSP
    CLI --> MCP
    CLI --> Runtime
    CLI --> API

    LSP --> MCP
    LSP --> API

    MCP --> API

    Runtime --> Java
    Runtime --> Gradle
    Runtime --> Core
    Runtime --> API

    Java --> Core
    Java --> API

    Gradle --> Core
    Gradle --> API

    Core --> API
```

Naviscope is built on a **layered crate architecture** that separates concerns across multiple Rust crates:

- **Interface Layer** (`naviscope-cli`, `naviscope-lsp`, `naviscope-mcp`): Entry points for different use cases (CLI shell, LSP for editors, MCP for AI agents).
- **Runtime Layer** (`naviscope-runtime`): Orchestrates the engine assembly, registering language plugins and providing a unified factory.
- **Language Layer** (`naviscope-java`, `naviscope-gradle`): Language-specific analysis plugins that parse and resolve symbols.
- **Core Layer** (`naviscope-core`): The heart of the system - graph storage, indexing, file scanning, and persistence.
- **API Layer** (`naviscope-api`): Common traits and models shared across all crates, ensuring a consistent interface.

The core is a language-agnostic graph structure populated by language-specific strategies (currently Java/Gradle via Tree-sitter), exposing a unified query engine to both AI agents and developer tools.

### 🔍 Reference Discovery Strategy

Naviscope uses a **two-phase reference discovery** approach for optimal performance:

1. **Meso-level (Coarse Filtering)**: Uses an inverted `reference_index` (token → files) to quickly identify candidate files that likely contain references to a symbol. This index is built during parsing by extracting all identifier tokens from source files.

2. **Micro-level (Precise Analysis)**: For each candidate file, uses Tree-sitter to parse and verify actual symbol occurrences, ensuring accurate reference locations.

This hybrid approach combines the speed of inverted indexing with the precision of syntax-aware parsing, enabling fast reference discovery even in large codebases.

## 🚀 Quick Start

### Prerequisites
- Rust (2024 edition)
- C Compiler (required for compiling Tree-sitter grammars)

### Installation from source code

```bash
# 1. Clone & Update Submodules (Required for tree-sitter grammars)
git clone https://github.com/biuld/naviscope.git
cd naviscope
git submodule update --init --recursive

# 2. Install the Naviscope CLI
cargo install --path .

# 3. (Optional) Build the VS Code Extension
cd editors/vscode
npm install
npm run package
# Then install the generated .vsix file in VS Code
```

### Usage

#### CLI Commands
- `naviscope index <PATH>`: Build a persistent index for a project.
- `naviscope shell [PATH]`: Start an interactive shell to query the graph.
- `naviscope watch <PATH>`: Start a background service to keep the index updated.
#### Configure in Cursor (for AI Agents)
1.  Open **Cursor Settings** (Cmd + Shift + J) -> **Features** -> **MCP**.
2.  Click **+ Add New MCP Server**.
3.  Configure:
    - **Name**: `Naviscope`
    - **Type**: `command`
    - **Command**: `naviscope mcp`

#### Configure in VS Code / NeoVim (for LSP)
- **VS Code**: Install the extension built in step 3.
- **Other Clients**: Point your LSP client to run `naviscope lsp`.

## 🛠️ Query DSL Examples

Whether using the CLI shell or MCP tools, the query logic is consistent:

```bash
# Find all classes named 'UserService'
find "UserService" --kind class

# List contents of a package
ls "com.example.service"

# Inspect full details of a symbol (source code, metadata)
cat "com.example.service.UserService"

# Find who references 'login'? (Incoming dependencies / Reverse lookups)
deps --rev "com.example.auth.AuthService.login"

# Filter dependencies by edge type
deps "com.example.User" --edge-types TypedAs,InheritsFrom
```

## 🔗 Graph Relationships

Naviscope tracks the following relationship types in the knowledge graph:

- **Structural**: `Contains` (package → class, class → method, etc.)
- **Inheritance**: `InheritsFrom`, `Implements`
- **Type Usage**: `TypedAs` (field/variable → type)
- **Annotations**: `DecoratedBy` (class/method → annotation)
- **Build System**: `UsesDependency` (project → dependency)

Reference discovery (method calls, instantiations) is handled efficiently through the `reference_index` + Tree-sitter two-phase approach, avoiding the need to store explicit call edges for every reference.

## 📈 Roadmap

- [x] **Core**: Graph Storage (`petgraph`), Parallel Indexing, Real-time Updates (`notify`).
- [x] **Languages**: Java & Gradle (Tree-sitter driven).
- [x] **Interfaces**: CLI Shell, MCP Server, LSP Server.
- [x] **Editors**: VS Code Extension.
- [x] **Reference Discovery**: Two-phase approach (reference_index + Tree-sitter).
- [ ] **Upcoming**: Maven Support, Python/Rust Language Strategies.

## 📄 License

This project is licensed under the MIT License - see the LICENSE file for details.
