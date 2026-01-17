# Naviscope 编码规范 (Coding Style Guide)

## 1. 核心理念

为了确保 `naviscope` 项目代码库的长期可维护性、可读性和健壮性，我们采纳以下核心编码理念。所有代码贡献都应遵循这些原则。

## 2. 函数式编程风格 (Functional Programming Style)

我们大力推崇并优先采用函数式编程（FP）风格。FP 有助于编写无副作用、易于测试、易于并行化和推理的代码。

*   **不可变性优先 (Immutability First)**
    *   默认所有变量和数据结构都应是不可变的。仅在有充分理由和性能考量时才使用 `mut` 关键字。
    *   倾向于创建数据的新实例，而不是在原地修改它。

*   **纯函数 (Pure Functions)**
    *   函数应尽量设计为纯函数：对于相同的输入，总是返回相同的输出，并且没有可观察的外部副作用（如修改全局状态、I/O 操作等）。

*   **善用迭代器和高阶函数 (Iterators and Higher-Order Functions)**
    *   对于集合操作，优先使用迭代器及其适配器（`map`, `filter`, `fold`, `and_then` 等），而不是手写的 `for` 或 `while` 循环。

    ```rust
    // Good:
    let squares = numbers.iter().map(|&x| x * x).collect::<Vec<_>>();
    ```

## 3. 类型驱动设计 (Type-Driven Design)

*   **代数数据类型 (ADT)**
    *   使用 `enum` 建模状态与变化（如 `GraphNode`, `EdgeType`）。
    *   利用编译器的穷尽性检查。

*   **Result 和 Option**
    *   禁止使用 `panic!`, `unwrap()`, `expect()` 处理可预见错误。
    *   使用 `Result<T, E>` 处理失败，`Option<T>` 处理可选值。

## 4. 第三方库规范 (Third-party Libraries)

我们使用最新的稳定库以获取性能和安全性的提升。

*   **错误处理 (`thiserror` 2.0+)**
    *   使用 `thiserror` 定义结构化错误。2.0+ 版本提供了更好的派生宏支持。
    ```rust
    #[derive(thiserror::Error, Debug)]
    pub enum MyError {
        #[error("IO error: {0}")]
        Io(#[from] std::io::Error),
    }
    ```

*   **序列化 (`serde` & `bincode` 2.0+)**
    *   使用 `serde` 进行属性定义。
    *   二进制序列化优先采用 `bincode` 2.0 (rc.3+)，它使用配置驱动的 API，比 1.x 更灵活且高性能。
    ```rust
    let config = bincode::config::standard();
    bincode::serde::encode_into_std_write(&value, &mut writer, config)?;
    ```

*   **语法解析 (`tree-sitter` 0.26+)**
    *   使用最新的 `tree-sitter` 绑定。注意 0.23+ 版本中 `QueryCursor` 的 API 变化，特别是 `matches` 返回值的迭代方式。

## 5. 项目工程规范

*   **代码格式化**: 必须通过 `cargo fmt`。
*   **质量检查**: 必须通过 `cargo clippy`。
*   **文档**: 所有的 `pub` 成员必须有 `rustdoc` 注释。
