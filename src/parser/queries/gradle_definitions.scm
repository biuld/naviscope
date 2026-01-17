;; Pattern for the dependencies block
(
    [
        (function_call function: (identifier) @name)
        (juxt_function_call function: (identifier) @name)
    ]
    (#eq? @name "dependencies")
) @dependencies_block

;; Pattern for dependency items
(
    [
        (function_call
            function: (identifier) @method_name
            args: (argument_list (string) @dep_string))
        (juxt_function_call
            function: (identifier) @method_name
            args: (argument_list (string) @dep_string))
    ]
    (#match? @method_name "^(implementation|api|testImplementation|compileOnly)$")
) @dependency_item
