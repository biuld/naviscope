# Runtime Orchestrator

## Purpose
The orchestrator wires plugins, builds the graph, runs background tasks, and serves queries. It is the control plane for the engine.

## Assembly Flow
```mermaid
flowchart TD
    Runtime[Orchestrator] --> Plugins[Collect Plugins]
    Runtime --> Core[Create Core Graph]
    Runtime --> Asset[Start Asset Service]
    Runtime --> Interfaces[Expose CLI/LSP/MCP]
```

## Lifecycle
- Build project graph first
- Start asset scanning in the background
- Serve queries immediately
- Upgrade placeholders as stubs arrive

## Injection Points
- AssetStubService
- Language plugin registry
- Cache managers
