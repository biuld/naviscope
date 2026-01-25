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

;; Pattern for project dependencies
(
    [
        (function_call
            function: (identifier) @method_name
            args: (argument_list
                (function_call
                    function: (identifier) @proj_fn
                    args: (argument_list (string) @project_path))))
        (juxt_function_call
            function: (identifier) @method_name
            args: (argument_list
                (function_call
                    function: (identifier) @proj_fn
                    args: (argument_list (string) @project_path))))
    ]
    (#match? @method_name "^(implementation|api|testImplementation|compileOnly)$")
    (#eq? @proj_fn "project")
) @project_dependency_item

;; Pattern for settings.gradle
(
    (assignment
        left: [
            (field_access
                object: (identifier) @obj
                field: (identifier) @field)
            (identifier) @field_id
        ]
        right: (string) @root_name)
    (#match? @obj "rootProject")
    (#match? @field "name")
    (#match? @field_id "rootProject.name")
) @root_project_assignment

(
    [
        (function_call
            function: (identifier) @include_fn
            args: (argument_list (string) @included_path))
        (juxt_function_call
            function: (identifier) @include_fn
            args: (argument_list (string) @included_path))
    ]
    (#eq? @include_fn "include")
) @include_call
