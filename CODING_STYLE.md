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
    *   有副作用的函数（如文件操作、网络请求）应被清晰地隔离出来。

*   **善用迭代器和高阶函数 (Iterators and Higher-Order Functions)**
    *   对于集合操作，优先使用迭代器及其适配器（`map`, `filter`, `fold`, `and_then` 等），而不是手写的 `for` 或 `while` 循环。这使得代码更具声明性，更少出错。

    ```rust
    // Good:
    let squares = numbers.iter().map(|&x| x * x).collect::<Vec<_>>();

    // Bad:
    let mut squares = vec![];
    for &x in &numbers {
        squares.push(x * x);
    }
    ```

## 3. 代数数据类型 (ADT) 与模式匹配 (Pattern Matching)

我们使用 Rust 强大的类型系统来建模领域逻辑，并确保代码的正确性。

*   **用 `enum` 建模状态与变化**
    *   `enum` 是我们用来表示不同状态、不同类型的实体或可能结果的核心工具。项目中的核心数据结构，如 `GraphNode`, `GraphQuery`, `EdgeType` 等，都应被定义为 `enum`。
    *   这使得我们可以利用编译器的穷尽性检查，确保所有可能的情况都得到了处理。

*   **拥抱 `match`**
    *   优先使用 `match` 语句进行控制流，而不是复杂的 `if let / else if` 链。`match` 强制我们处理所有情况，使代码更安全、更易读。

    ```rust
    // Good:
    match node {
        GraphNode::Code(c) => handle_code(c),
        GraphNode::Build(b) => handle_build(b),
    }

    // Avoid if a match would be clearer:
    if let GraphNode::Code(c) = node {
        handle_code(c)
    } else if let GraphNode::Build(b) = node {
        handle_build(b)
    }
    ```

*   **`Result` 和 `Option` 是处理结果和可选值的唯一方式**
    *   所有可能失败的操作必须返回 `Result<T, E>`。
    *   所有可能不存在的值必须使用 `Option<T>`。
    *   严禁在库代码中使用 `panic!` 来处理可预见的错误。`unwrap()` 和 `expect()` 只能在示例代码或确信逻辑上不可能失败的地方使用。

## 4. 其他规范

*   **错误处理 (Error Handling)**
    *   使用 `thiserror` crate 来定义清晰、结构化的错误类型枚举。这有助于错误的传播和处理。

*   **代码格式化 (Formatting)**
    *   所有提交的代码 **必须** 使用 `rustfmt` 进行格式化。可以使用 `cargo fmt` 命令来自动格式化整个项目。

*   **代码质量检查 (Linting)**
    *   所有提交的代码 **必须** 通过 `clippy` 的默认检查（`cargo clippy`）。应修复所有 Clippy 报告的警告，除非有充分理由将其禁用。

*   **注释 (Comments)**
    *   只在必要时添加注释，重点解释代码的“为什么”，而不是“做什么”。
    *   为所有 `pub` 的函数、结构体、枚举和 `trait` 编写符合 `rustdoc` 规范的文档注释。
