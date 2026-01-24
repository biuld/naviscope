# Naviscope for VS Code

Naviscope is a graph-based, structured code query engine that powers your coding experience with deep semantic understanding. This extension brings the power of Naviscope's Language Server Protocol (LSP) integration directly into VS Code, offering a lightweight and blazing-fast alternative for Java navigation.

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
- **Zero JVM Overhead**: No heavy background processes or memory-hogging language servers.
- **Resilient Indexing**: Works effectively even with syntax errors or incomplete code.
- **Unified Graph**: Built on the same engine used by LLM agents via MCP.

## 📦 System Requirements

- **Linux** (x86_64)
- **macOS** (Apple Silicon / ARM64)
- *Windows and Intel macOS are not currently supported.*

## 📦 Installation

Just install the extension!

On first launch, Naviscope will automatically detect your platform and download the necessary binary engine to `~/.naviscope/bin`. It's completely managed by the extension—no manual configuration required.

## 🔧 Troubleshooting

If the extension fails to start:
1.  Check your internet connection if this is the first run (the binary needs to be downloaded).
2.  Check the "Naviscope Client" output channel in VS Code for detailed logs.

## 🗑️ Uninstallation

Uninstalling the extension does not automatically remove the cached binary and index data. To completely remove Naviscope from your system:

1.  Uninstall the extension from VS Code.
2.  Delete the `~/.naviscope` directory manually.

## 🔗 Links

-   [Naviscope Repository](https://github.com/biuld/naviscope)
-   [Issue Tracker](https://github.com/biuld/naviscope/issues)

## 📄 License

MIT License
