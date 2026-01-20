# Naviscope

Naviscope is a graph-based, structured code query engine specifically designed for Large Language Models (LLMs). It builds a comprehensive **Code Knowledge Graph** that bridges the gap between micro-level source code semantics (calls, inheritance) and macro-level project structures (modules, packages, dependencies).

Unlike traditional text search, Naviscope provides a deep, structured understanding of your codebase, enabling LLMs to navigate and reason about complex software systems with precision.

## 🌟 Key Features

- **Code Knowledge Graph**: Represents project entities and their complex relationships in a unified graph using `petgraph`.
- **LLM-Friendly DSL**: A shell-like query interface (`grep`, `ls`, `inspect`, `incoming`, `outgoing`) that returns structured JSON data optimized for LLM agents.
- **High-Performance Indexing**: A robust 3-phase processing pipeline (Scan & Parse → Resolve → Apply) utilizing Rust's concurrency for maximum speed.
- **Real-time Synchronization**: Automatic graph updates via file system watching (`notify`), ensuring the index stays consistent with your changes.
- **Multi-Protocol Interface**: Support for both **MCP** (Model Context Protocol) for AI agents and **LSP** (Language Server Protocol) for IDEs.
- **Extensible Architecture**: Language-neutral core with a strategy-based resolver. Currently focused on **Java + Gradle**, with Maven support in progress.

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
- `naviscope query <PATH> <JSON>`: Execute a structured DSL query manually.
- `naviscope watch <PATH>`: Start a background service to keep the index updated as you edit files.
- `naviscope schema`: Display the JSON schema and examples for the GraphQuery DSL.
- `naviscope clear [PATH]`: Remove specific project index or clear all cached indices.
- `naviscope mcp`: Start the Model Context Protocol server.
- `naviscope lsp`: Start the Language Server Protocol server.

### Model Context Protocol (MCP) Support
Naviscope implements the [Model Context Protocol (MCP)](https://modelcontextprotocol.io/), allowing LLM agents like Cursor and Claude to directly use its code knowledge graph.

#### Available Tools
- **`grep`**: Global search for symbols by pattern and kind (Class, Method, etc.).
- **`ls`**: List package members, class fields, or project modules.
- **`inspect`**: Retrieve full metadata and source code for a specific Fully Qualified Name (FQN).
- **`incoming`**: Trace inbound relationships like callers, implementers, or references.
- **`outgoing`**: Trace outbound relationships like callees or class dependencies.

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
- **Zero JVM Overhead**: No more "Java Language Server is indexing..." hanging your UI.
- **Resilient**: Works even if your code has syntax errors or missing dependencies.
- **Unified Knowledge**: Shares the same core graph used by MCP for LLM agents.

## 🛠️ Query API Examples

The Query DSL (used by `naviscope query` and MCP) supports several commands for structured exploration:
- `grep`: `{"command": "grep", "pattern": "UserService", "kind": ["class"]}`
- `ls`: `{"command": "ls", "fqn": "com.example.service"}`
- `inspect`: `{"command": "inspect", "fqn": "com.example.service.UserService"}`
- `incoming`: `{"command": "incoming", "fqn": "com.example.service.UserService#save", "edge_type": ["Calls"]}`
- `outgoing`: `{"command": "outgoing", "fqn": "com.example.service.UserService"}`

## 📈 Roadmap (V1)
- [x] Core Graph Storage (`petgraph`)
- [x] Java & Gradle Parser (Tree-sitter driven)
- [x] Shell-like Query DSL Engine
- [x] Parallel Indexing & Real-time Updates (`notify`)
- [x] MCP Server implementation
- [x] LSP Support (Definition, References, Hierarchy, Hover, etc.)
- [x] VSCode Extension (Initial version)
- [ ] Maven Support (In Progress)
- [ ] Python/Rust Language Strategies (Planned)

## 📄 License
This project is licensed under the MIT License - see the LICENSE file for details.
