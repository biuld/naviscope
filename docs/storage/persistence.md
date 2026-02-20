# Storage and Persistence

## Purpose
Persistence enables fast restarts and cross-session reuse of graph data and external stubs.

## On-Disk Layout
- Project index storage
- Global stub cache

## Formats and Versioning
```mermaid
flowchart LR
    Data[Index Data] --> Encode[Encode]
    Encode --> Store[Store]
    Store --> Load[Load]
    Load --> Decode[Decode]
```

## Migration Strategy
- Versioned metadata for safe upgrades
- Fallback read for older formats
- Progressive migrations where possible
