# Java Language Strategy

## Parsing
- Tree-sitter based source parsing
- Package and type extraction
- Identifier indexing for reference discovery

## Bytecode Stubs
```mermaid
flowchart TD
    ClassFile[.class] --> Parse[Bytecode Parser]
    Parse --> Stub[IndexNode Stub]
    Stub --> Graph[Graph Update]
```

## Inheritance and Type System
- Class and interface relations
- Field and method signatures
- Access modifiers and annotations

## Edge Cases
- Inner classes
- Generics and annotations
- Split packages across multiple JARs
