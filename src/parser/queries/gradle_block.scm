(
    [
        (function_call function: (identifier) @name)
        (juxt_function_call function: (identifier) @name)
    ]
    (#eq? @name "dependencies")
) @dependencies_block
