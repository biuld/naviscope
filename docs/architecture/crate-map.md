# Crate Map

## Layered Structure
```mermaid
graph TD
    API[naviscope-api]
    Plugin[naviscope-plugin]
    Core[naviscope-core]
    Runtime[naviscope-runtime]
    Java[naviscope-java]
    Gradle[naviscope-gradle]
    CLI[naviscope-cli]
    LSP[naviscope-lsp]
    MCP[naviscope-mcp]

    CLI --> Runtime
    LSP --> Runtime
    MCP --> Runtime

    Runtime --> Core
    Runtime --> Java
    Runtime --> Gradle
    Runtime --> Plugin

    Java --> Plugin
    Gradle --> Plugin
    Core --> Plugin

    Plugin --> API
    Core --> API
    Runtime --> API
    Java --> API
    Gradle --> API
    CLI --> API
    LSP --> API
    MCP --> API
```

## What Each Layer Provides
- **API**: shared models plus engine service traits (`GraphService`, `NavigationService`, semantic traits, `EngineLifecycle`) and the composite `NaviscopeEngine`.
- **Plugin**: capability traits (`*Cap` + runtime semantic services) for language/build integrations; keeps Core independent.
- **Core**: graph storage, indexing, persistence, and asset services.
- **Runtime**: orchestration, lifecycle, background tasks, and query serving.
- **Language/Build**: concrete strategies (Java parsing, Gradle structure resolution).
- **Interfaces**: CLI/LSP/MCP entry points that expose the same graph.

## Flow Through Crates
1. Interfaces call Runtime to build an index or run a query.
2. Runtime invokes language/build plugins to parse and resolve structure.
3. Plugins emit nodes/edges into Core Graph.
4. Runtime serves queries from Core Graph and coordinates background stubbing.
