# Reference Discovery

## Two-Phase Strategy
```mermaid
flowchart LR
    Tokens[Token Index] --> Candidates[Candidate Files]
    Candidates --> Parse[Syntax Parse]
    Parse --> Matches[Verified References]
```

## Meso-Level Filtering
- Build a token-to-file inverted index.
- Cheap coarse filtering reduces search space.
- Keeps reference queries fast on large repos.

## Micro-Level Validation
- Tree-sitter parsing for precise matches.
- Symbol-aware filtering by scope and type.
- Avoids false positives from raw text search.

## Practical Flow
1. Token index returns a candidate file set.
2. Each candidate is parsed for real symbol usage.
3. Verified results are returned to the caller.
