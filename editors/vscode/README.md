# Naviscope for VS Code

**Unified Code Knowledge Graph Engine for Developers**

Naviscope bridges the gap between AI and IDEs. This extension brings the power of Naviscope's **Unified Code Knowledge Graph** directly into VS Code, offering a lightweight, lightning-fast alternative to traditional Java language servers (like JDTLS).

It shares the **exact same graph representation** used by AI agents (via MCP), ensuring that what your AI assistant sees is exactly what you navigate in the editor.

## 💡 Why Naviscope?

| Feature | Traditional Tools (JDTLS) | Naviscope |
| :--- | :--- | :--- |
| **Performance** | High latency, heavy memory (JVM) | **Instant**, minimal footprint (Rust) |
| **Resilience** | Blocks on build errors | **Robust**, works with partial code |
| **Context** | Separate from AI tools | **Unified** with AI Agents (MCP) |

## 🌟 Capabilities

### ⚡️ Blazing Fast Navigation
Instant symbol resolution without the overhead of a full JVM-based language server.

- **Go to Definition**: Jump to symbol definitions instantly.
- **Find References**: Global reference tracking across the entire workspace.
- **Go to Implementation**: Find interface implementations and method overrides.
- **Go to Type Definition**: Navigate to the type of a variable or parameter.

### 🧠 Intelligent Code Understanding
- **Call Hierarchy**: Visualize incoming and outgoing method calls.
- **Workspace Symbols**: Fuzzy search for any class, method, or field.
- **Document Symbols**: Quickly outline and navigate the current file structure.
- **Hover Information**: View signatures and documentation on hover.
- **Document Highlights**: Highlight all local references of a symbol.

## 📦 Installation

> **Prerequisite**: You must have the `naviscope` CLI installed on your system.

1.  **Install the CLI**:
    Follow the instructions in the [main repository](https://github.com/biuld/naviscope) to install `naviscope` via Cargo:
    ```bash
    git clone https://github.com/biuld/naviscope.git
    cd naviscope
    cargo install --path .
    ```

2.  **Install the Extension**:
    - Download the `.vsix` release or build from source.
    - Install via **Extensions** view -> **...** -> **Install from VSIX...**

3.  **Configure**:
    Ensure `naviscope` is in your `PATH`. If not, set the path in VS Code settings:
    ```json
    "naviscope.path": "/path/to/your/naviscope/binary"
    ```

4.  **Start Coding**:
    Open any Java/Gradle project. Naviscope will automatically start indexing (you'll see a status bar item).

## 📦 System Requirements

- **macOS** (Apple Silicon / ARM64)
- **Linux** (x86_64)
- *Windows support is planned.*

## 🔧 Troubleshooting

If the extension fails to start:
1.  Verify `naviscope --version` works in your terminal.
2.  Check the **Output** panel (select "Naviscope Client" from the dropdown).
3.  Ensure your project root contains a `build.gradle` or similar marker (if required).

## 🔗 Links

- [Naviscope Main Repository](https://github.com/biuld/naviscope)
- [Report an Issue](https://github.com/biuld/naviscope/issues)

## 📄 License
MIT License
