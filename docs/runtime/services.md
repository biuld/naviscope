# Runtime Services

## AssetStubService
- Coordinates discovery, indexing, and stub requests
- Provides route lookup for external symbols
- Emits stub results to graph updates

## Indexing Service
- Triggers parsing and graph build
- Manages incremental updates
- Handles error isolation per file

## Cache Service
- Handles global stub cache
- Supports cache statistics and inspection

## Other Runtime Services
- File watching
- Diagnostics and logging
