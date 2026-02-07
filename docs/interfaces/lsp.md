# LSP Interface

## Purpose
LSP exposes navigation and insight features to editors using the same graph as CLI/MCP.

## Supported Capabilities
- Definition, references, implementation
- Hover, symbols, hierarchy

## Request/Response Mapping
```mermaid
sequenceDiagram
    participant Client
    participant LSP
    participant Runtime
    participant Graph

    Client->>LSP: textDocument/definition
    LSP->>Runtime: resolve symbol
    Runtime->>Graph: query
    Graph-->>LSP: results
    LSP-->>Client: locations
```

## Performance Considerations
- Avoid full re-index on open
- Serve from graph cache first
