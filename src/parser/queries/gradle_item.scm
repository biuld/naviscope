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
)
