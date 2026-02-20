# System Overview

## Purpose
Naviscope builds a unified code knowledge graph that serves both AI agents (MCP) and developer tools (LSP/CLI). The system is designed for fast startup, incremental enrichment, and consistent semantics across interfaces.

## High-Level Architecture
```mermaid
graph TD
    subgraph Interfaces
        CLI[CLI] --> Runtime
        LSP[LSP Server] --> Runtime
        MCP[MCP Server] --> Runtime
    end

    subgraph Runtime
        Runtime[Naviscope Engine / Orchestrator]
    end

    subgraph Language
        Java[Java Strategy]
        Gradle[Gradle Strategy]
    end

    subgraph Core
        CoreGraph[Core Graph + Indexing]
        Asset[Asset/Stub Service]
    end

    subgraph Plugins
        Contracts[Plugin Contracts]
    end

    Runtime --> CoreGraph
    Runtime --> Language
    Language --> Contracts
    CoreGraph --> Contracts
    Runtime --> Asset
```

## Main Flow (Index + Query)
```mermaid
sequenceDiagram
    participant UI as Interfaces
    participant RT as Runtime Orchestrator
    participant LG as Language Plugins
    participant CG as Core Graph
    participant AS as Asset/Stub Service

    UI->>RT: index/project open
    RT->>LG: parse + resolve modules
    LG->>CG: emit nodes/edges
    RT->>AS: scan assets (async)
    AS-->>CG: enrich external nodes (stubs)
    UI->>RT: query (find/ls/cat)
    RT->>CG: graph query
    CG-->>UI: results
```

## How to Read the System
- The **Runtime Orchestrator** is the control plane: it wires plugins, triggers indexing, and serves queries.
- The **Core Graph** is the single source of truth: all interfaces read from it.
- The **Asset/Stub Service** enriches external symbols without blocking startup.
- **Plugins** supply language/build-tool-specific logic; Core stays language-agnostic.

## Key Design Expectations
- Indexing should be usable even if external dependencies are still scanning.
- External symbols must be represented as first-class nodes, not special cases.
- The same symbol must have a stable `NodeId` across source and bytecode.
