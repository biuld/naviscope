# MCP Interface

## Purpose
MCP exposes graph queries to AI agents via a stable tool surface. It is optimized for structured, precise context delivery.

## Tooling Surface
- `get_guide`
- `ls`, `find`, `cat`, `deps`

## Usage Flow
```mermaid
sequenceDiagram
    participant Agent
    participant MCP
    participant Runtime

    Agent->>MCP: find "Symbol"
    MCP->>Runtime: query graph
    Runtime-->>MCP: results
    MCP-->>Agent: structured response
```

## Safety and Limits
- Read-only by default
- Resource limits per request
