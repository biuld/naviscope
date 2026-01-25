# Naviscope

Naviscope is a **unified Code Knowledge Graph engine** that bridges the gap between AI agents and developers. It builds a comprehensive graph representation of your codebase, connecting micro-level source code semantics (calls, inheritance) with macro-level project structures (modules, packages, dependencies).

Unlike traditional text search or language servers, Naviscope provides a **single, unified knowledge graph** that powers both LLM agents (via MCP) and IDE features (via LSP), enabling precise code navigation and reasoning across complex software systems.

## 🌟 Key Features

- **Unified Code Knowledge Graph**: A single graph representation using `petgraph` that powers both MCP (for LLMs) and LSP (for IDEs), ensuring consistency across all tools.
- **Zero JVM Overhead**: Built entirely in Rust, providing instant startup and low memory footprint—no more waiting for Java language servers to index.
- **LLM-Optimized Query Interface**: Structured JSON responses via MCP tools (`grep`, `ls`, `inspect`, `deps`) designed specifically for AI agent consumption.
- **High-Performance Indexing**: A robust 3-phase processing pipeline (Scan & Parse → Resolve → Apply) utilizing Rust's concurrency for maximum speed.
- **Real-time Synchronization**: Automatic graph updates via file system watching (`notify`), ensuring the index stays consistent with your changes.
- **Dual Protocol Support**: Native **MCP** (Model Context Protocol) for AI agents and **LSP** (Language Server Protocol) for IDEs, both sharing the same underlying graph.
- **Resilient & Fast**: Works effectively even with syntax errors or missing dependencies, providing immediate feedback without blocking on incomplete code.
- **Extensible Architecture**: Language-neutral core with a strategy-based resolver. Currently supports **Java + Gradle**, with Maven support in progress.

## 🏗️ Architecture

Naviscope processes code through three distinct phases:
1.  **Phase 1: Scan & Parse**: Parallel file scanning and AST extraction using Tree-sitter. It captures raw definitions and references at the file level.
2.  **Phase 2: Resolve**: Bridges the "semantic gap" by mapping raw entities to logical project structures (Modules/Packages) and resolving symbols across the workspace.
3.  **Phase 3: Apply**: Merges resolved operations into a `StableDiGraph` and persists the index for fast subsequent access.

## 🚀 Quick Start

### Prerequisites
- Rust (2024 edition)
- C compiler (required for compiling Tree-sitter grammars)

### Installation
```bash
# 1. Update submodules (required for tree-sitter grammars)
git submodule update --init --recursive

# 2. Install the Naviscope CLI
cargo install --path .

# 3. Build the VS Code Extension (Optional)
cd editors/vscode
npm install
npm run package
# Install the generated .vsix in VS Code
```

### CLI Commands
- `naviscope index <PATH>`: Build a persistent index for a project (stored in `~/.naviscope/indices`).
- `naviscope shell [PATH]`: Start an interactive shell to query the code knowledge graph.
- `naviscope watch <PATH>`: Start a background service to keep the index updated as you edit files.
- `naviscope schema`: Display the JSON schema and examples for the GraphQuery DSL.
- `naviscope clear [PATH]`: Remove specific project index or clear all cached indices.
- `naviscope mcp`: Start the Model Context Protocol server.
- `naviscope lsp`: Start the Language Server Protocol server.

### Model Context Protocol (MCP) Support
Naviscope implements the [Model Context Protocol (MCP)](https://modelcontextprotocol.io/), allowing LLM agents like Cursor and Claude to directly use its code knowledge graph.

#### Available Tools
- **`grep`**: Global search for symbols by pattern and kind (Class, Method, etc.) across the entire project.
- **`ls`**: List package members, class fields, or project modules. Explore project structure hierarchically.
- **`inspect`**: Retrieve full metadata and source code for a specific Fully Qualified Name (FQN).
- **`deps`**: Analyze dependencies—outgoing (what I depend on) or incoming (who depends on me) with optional edge type filtering.

#### Configuring in Cursor
To use Naviscope in Cursor:
1. Open **Cursor Settings** (Cmd + Shift + J on macOS).
2. Navigate to **Features** -> **MCP**.
3. Click **+ Add New MCP Server**.
4. Configure as follows:
   - **Name**: `Naviscope`
   - **Type**: `command`
   - **Command**: `naviscope mcp`
5. Click **Save**. Cursor will now automatically index your project when you ask questions.

#### Configuring in Claude Desktop
Add the following entry to your `claude_desktop_config.json`:
```json
{
  "mcpServers": {
    "naviscope": {
      "command": "naviscope",
      "args": ["mcp"]
    }
  }
}
```

### Language Server Protocol (LSP) Support
Naviscope acts as a high-performance LSP server for Java, offering a lightweight alternative to JDTLS. It uses precise semantic edge tracking to ensure high availability and accuracy.

#### Features
- **Go to Definition**: Instant jump to symbol definitions using precise edge tracking.
- **Find References**: Global reference tracking across the entire project graph.
- **Call Hierarchy**: Visualize incoming and outgoing method calls.
- **Go to Type Definition**: Jump to the class definition of variables or return types.
- **Go to Implementation**: Find all implementations of an interface or overrides of a method.
- **Document Symbol**: Navigate class structures (classes, methods, fields) within a file.
- **Workspace Symbol**: Global fuzzy search for classes and methods.
- **Hover**: View symbol signatures and documentation snippets.
- **Document Highlight**: Highlight all local references of a symbol in the current document.

#### Usage in VSCode / NeoVim
- **VSCode**: 
  1. Build the `.vsix` as shown in the Installation section.
  2. Install it in VS Code via "Install from VSIX...".
  3. Ensure the `naviscope` binary is in your PATH or configure `naviscope.path` in settings.
- **Other Editors**: Simply point your LSP client to the `naviscope lsp` command.

#### Why Naviscope LSP?
- **Zero JVM Overhead**: Built in Rust, providing instant startup and low memory usage—no more "Java Language Server is indexing..." blocking your workflow.
- **Resilient**: Works effectively even with syntax errors or missing dependencies, providing immediate navigation without waiting for perfect code.
- **Unified Knowledge Graph**: Shares the exact same core graph used by MCP for LLM agents, ensuring consistency between AI-assisted development and manual navigation.
- **Lightweight Alternative**: A fast, memory-efficient replacement for JDTLS that doesn't require a full Java runtime.

## 🛠️ Query API Examples

The Query DSL (used by `naviscope shell` and MCP) supports several commands for structured exploration:

**Shell Commands:**
```bash
grep "UserService"                    # Search for symbols matching pattern
ls "com.example.service"              # List package contents
cat "com.example.service.UserService" # Inspect full details of a symbol
deps "com.example.service.UserService" # Show dependencies (outgoing by default)
deps --rev "com.example.service.UserService" # Show reverse dependencies (incoming)
```

**JSON DSL (for MCP/API):**
```json
{"command": "grep", "pattern": "UserService", "kind": ["class"], "limit": 20}
{"command": "ls", "fqn": "com.example.service", "kind": ["class", "interface"]}
{"command": "cat", "fqn": "com.example.service.UserService"}
{"command": "deps", "fqn": "com.example.service.UserService", "rev": false, "edge_type": ["Calls", "InheritsFrom"]}
```

## 🎯 Project Positioning

Naviscope fills a unique niche in the developer tooling ecosystem:

**For LLM Agents**: Provides structured, graph-based code understanding that goes far beyond text search, enabling AI assistants to reason about code relationships, dependencies, and architecture.

**For Developers**: Offers a lightweight, fast LSP server that doesn't require JVM overhead, with the added benefit of sharing the same knowledge graph that powers AI-assisted development.

**The Unified Advantage**: Unlike traditional tools that maintain separate indexes for different purposes, Naviscope's single knowledge graph ensures that what AI agents see is exactly what developers navigate—creating a seamless, consistent experience across all development workflows.

## 📈 Roadmap (V1)
- [x] Core Graph Storage (`petgraph`)
- [x] Java & Gradle Parser (Tree-sitter driven)
- [x] Shell-like Query DSL Engine
- [x] Parallel Indexing & Real-time Updates (`notify`)
- [x] MCP Server implementation (grep, ls, inspect, deps)
- [x] LSP Support (Definition, References, Hierarchy, Hover, etc.)
- [x] VSCode Extension
- [ ] Maven Support (In Progress)
- [ ] Python/Rust Language Strategies (Planned)

## 📄 License
This project is licensed under the MIT License - see the LICENSE file for details.
