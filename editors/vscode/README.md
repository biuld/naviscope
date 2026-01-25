# Naviscope for VS Code

Naviscope is a unified Code Knowledge Graph engine that powers both AI agents and developers. This VS Code extension brings Naviscope's Language Server Protocol (LSP) integration directly into your editor, offering a lightweight, blazing-fast alternative to JDTLS for Java navigation.

## 🚀 Features

### Blazing Fast Navigation
Naviscope builds a comprehensive Code Knowledge Graph of your project, enabling instant symbol resolution without the overhead of a full JVM-based language server.

- **Go to Definition**: Jump to symbol definitions instantly.
- **Find References**: See usages across your entire workspace.
- **Go to Implementation**: Find interface implementations and method overrides.
- **Go to Type Definition**: Navigate to the type of a variable or parameter.

### Intelligent Code Understanding
- **Call Hierarchy**: Explore incoming and outgoing calls to understand code flow.
- **Workspace Symbols**: Fuzzy search for any class, method, or field in your project.
- **Document Symbols**: Quickly outline and navigate the current file structure.
- **Hover Information**: View signatures and documentation on hover.
- **Document Highlights**: Highlight all occurrences of a symbol in the current file.

### Why Use Naviscope?
- **Zero JVM Overhead**: Built entirely in Rust—no Java runtime required, instant startup, minimal memory footprint.
- **Resilient Indexing**: Works effectively even with syntax errors or incomplete code, providing immediate navigation without waiting for perfect builds.
- **Unified Knowledge Graph**: Shares the exact same code knowledge graph used by LLM agents via MCP, ensuring consistency between AI-assisted development and manual navigation.
- **Lightweight Alternative**: A fast, memory-efficient replacement for JDTLS that doesn't block your workflow.

## 📦 System Requirements

- **Linux** (x86_64)
- **macOS** (Apple Silicon / ARM64)
- *Windows and Intel macOS are not currently supported.*

## 📦 Installation

1. Install the extension from the VS Code marketplace or from a `.vsix` file.
2. Ensure the `naviscope` binary is available in your PATH, or configure the `naviscope.path` setting to point to the binary location.
3. Open a Java project—Naviscope will automatically start indexing your workspace.

**Note**: You need to have the `naviscope` CLI installed separately. See the [main repository](https://github.com/biuld/naviscope) for installation instructions.

## 🔧 Troubleshooting

If the extension fails to start:
1. Ensure the `naviscope` binary is installed and available in your PATH, or configure `naviscope.path` in VS Code settings.
2. Check the "Naviscope Client" output channel in VS Code for detailed logs.
3. Verify your project is a valid Java/Gradle project structure.

## 🗑️ Uninstallation

Uninstalling the extension does not automatically remove the cached binary and index data. To completely remove Naviscope from your system:

1.  Uninstall the extension from VS Code.
2.  Delete the `~/.naviscope` directory manually.

## 🔗 Links

-   [Naviscope Repository](https://github.com/biuld/naviscope)
-   [Issue Tracker](https://github.com/biuld/naviscope/issues)

## 📄 License

MIT License
