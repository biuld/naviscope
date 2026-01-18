# Naviscope

Naviscope is a graph-based, structured code query engine specifically designed for Large Language Models (LLMs). It builds a comprehensive **Code Knowledge Graph** that bridges the gap between micro-level source code semantics (calls, inheritance) and macro-level project structures (modules, packages, dependencies).

Unlike traditional text search, Naviscope provides a deep, structured understanding of your codebase, enabling LLMs to navigate and reason about complex software systems with precision.

## 🌟 Key Features

- **Code Knowledge Graph**: Represents project entities and their complex relationships in a unified graph using `petgraph`.
- **LLM-Friendly DSL**: A shell-like query interface (e.g., `grep`, `ls`, `inspect`, `incoming`) that returns structured JSON data optimized for LLM agents.
- **High-Performance Indexing**: A robust 3-phase processing pipeline (Scan & Parse → Resolve → Apply) utilizing Rust's concurrency for maximum speed.
- **Incremental Updates**: Real-time graph synchronization via file system watching (`notify`), ensuring the graph stays up-to-date with your changes.
- **Extensible Architecture**: Language-neutral core with a strategy-based resolver. Currently focused on **Java + Gradle (V1)**, with plans for Rust, Python, and more.

## 🏗️ Architecture

Naviscope processes code through three distinct phases:
1.  **Phase 1: Scan & Parse**: Parallel file scanning and AST extraction using Tree-sitter grammars.
2.  **Phase 2: Resolve**: Bridging the "semantic gap" by mapping raw entities to logical project structures (Modules/Packages) and generating idempotent graph operations.
3.  **Phase 3: Apply**: Efficiently merging operations into a `StableDiGraph` for persistent indexing.

## 🚀 Quick Start

### Prerequisites
- Rust (2024 edition)
- C compiler (required for compiling Tree-sitter grammars)

### Installation
```bash
cargo install --path .
```

### Basic Usage
```bash
# Build an index for a Java project (stored automatically in ~/.naviscope/indices)
naviscope index /path/to/java-project

# Execute a structured query on the indexed project
naviscope query /path/to/java-project '{"command": "grep", "pattern": "UserService", "kind": ["Class"]}'
```

### Model Context Protocol (MCP) Support
Naviscope implements the [Model Context Protocol (MCP)](https://modelcontextprotocol.io/), allowing LLM agents like Cursor and Claude to directly use its code knowledge graph.

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

## 🛠️ Query API Examples

For LLM agents, Naviscope exposes commands designed for structured exploration:
- `grep`: Global search for symbols by pattern and kind.
- `ls`: List members (methods/fields) of a class or package.
- `inspect`: Retrieve full metadata for a specific Fully Qualified Name (FQN).
- `incoming`: Trace callers, implementers, or other inbound relationships.
- `outgoing`: Trace callees, dependencies, or outbound relationships.

## 📈 Roadmap (V1)
- [x] Core Graph Storage (`petgraph`)
- [x] Java & Gradle Parser (Tree-sitter driven)
- [x] Shell-like Query DSL Engine
- [x] Parallel Indexing & Incremental Updates
- [ ] Maven Support (Coming Soon)
- [ ] Python/Rust Language Strategies (Planned)

## 📄 License
This project is licensed under the MIT License - see the LICENSE file for details.
