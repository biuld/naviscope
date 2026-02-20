# Source Resolution

## Purpose
Source resolution attaches real source code to external symbols at query time, improving navigation and documentation visibility.

## Flow
```mermaid
flowchart TD
    Query[Go to Definition] --> Lookup[Locate sources.jar]
    Lookup --> Parse[Parse Source]
    Parse --> Return[Return Rich Source]
```

## Relationship to Stubs
- Stubs provide structure and fast navigation
- Source parsing adds comments and full body
- Fallback to stubs when sources are missing

## UX Notes
- On-demand only to avoid startup cost
- Clear messaging when sources are unavailable
