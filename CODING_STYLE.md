# Naviscope Coding Style Guide

## 1. Core Philosophy

To ensure the long-term maintainability, readability, and robustness of the `naviscope` project, we adopt the following core coding philosophies. All contributions are expected to follow these principles.

## 2. Functional Programming Style

We strongly advocate and prioritize Functional Programming (FP) patterns. FP helps write side-effect-free, easily testable, parallelizable, and easy-to-reason-about code.

*   **Immutability First**
    *   By default, all variables and data structures should be immutable. Use the `mut` keyword only when there is a strong justification and performance considerations.
    *   Prefer creating new instances of data rather than modifying it in place.

*   **Pure Functions**
    *   Design functions to be as pure as possible: for the same input, they should always return the same output, with no observable external side effects (e.g., modifying global state, performing I/O).

*   **Iterators and Higher-Order Functions**
    *   For collection operations, prefer using iterators and their adapters (`map`, `filter`, `fold`, `and_then`, etc.) over manual `for` or `while` loops.

    ```rust
    // Good:
    let squares = numbers.iter().map(|&x| x * x).collect::<Vec<_>>();
    ```

## 3. Type-Driven Design

*   **Algebraic Data Types (ADTs)**
    *   Use `enum` to model states and variants (e.g., `GraphNode`, `EdgeType`).
    *   Leverage the compiler's exhaustiveness checking.

*   **Result and Option**
    *   Do not use `panic!`, `unwrap()`, or `expect()` for predictable error handling.
    *   Use `Result<T, E>` for failure handling and `Option<T>` for optional values.

## 4. Third-party Library Standards

We use the latest stable libraries to benefit from performance and security improvements.

*   **Error Handling (`thiserror` 2.0+)**
    *   Use `thiserror` to define structured errors. Version 2.0+ provides better derive macro support.
    ```rust
    #[derive(thiserror::Error, Debug)]
    pub enum MyError {
        #[error("IO error: {0}")]
        Io(#[from] std::io::Error),
    }
    ```

*   **Serialization (`serde` & `bincode` 2.0+)**
    *   Use `serde` for attribute definitions.
    *   For binary serialization, prefer `bincode` 2.0 (rc.3+), which uses a configuration-driven API that is more flexible and higher-performing than 1.x.
    ```rust
    let config = bincode::config::standard();
    bincode::serde::encode_into_std_write(&value, &mut writer, config)?;
    ```

*   **Parsing (`tree-sitter` 0.26+)**
    *   Use the latest `tree-sitter` bindings. Be mindful of API changes in `QueryCursor` in versions 0.23+, especially regarding how match iterators are handled.

## 5. Engineering Standards

*   **Code Formatting**: All code must pass `cargo fmt`.
*   **Linting**: All code must pass `cargo clippy`.
*   **Documentation**: All `pub` members must have `rustdoc` comments.
