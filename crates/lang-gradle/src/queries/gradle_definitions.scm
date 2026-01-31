;; Pattern for the dependencies block
(
    [
        (function_call function: (identifier) @name)
        (juxt_function_call function: (identifier) @name)
    ]
    (#eq? @name "dependencies")
) @dependencies_block

;; Pattern for dependency items (External libraries)
(
    [
        (function_call
            function: (identifier) @method_name
            args: (argument_list (string) @dep_string))
        (juxt_function_call
            function: (identifier) @method_name
            args: (argument_list (string) @dep_string))
    ]
    (#match? @method_name "^(implementation|api|testImplementation|compileOnly|runtimeOnly|annotationProcessor)$")
) @dependency_item

;; Pattern for project dependencies
(
    [
        ;; Normal nested call: implementation project(':core')
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
        
        ;; Split AST (seen in some grammars): implementation project (':core')
        (
            (juxt_function_call
                function: (identifier) @method_name
                args: (argument_list (identifier) @proj_fn))
            (parenthesized_expression (string) @project_path)
            (#eq? @proj_fn "project")
        )
    ]
    (#match? @method_name "^(implementation|api|testImplementation|compileOnly|runtimeOnly|annotationProcessor)$")
    (#eq? @proj_fn "project")
) @project_dependency_item

;; Pattern for settings.gradle: rootProject.name = '...'
;; Handles both assignment and method calls
(
    [
        (assignment
            (dotted_identifier
                (identifier) @obj
                (identifier) @prop)
            (string) @root_name)
        (function_call
            function: (dotted_identifier
                (identifier) @obj
                (identifier) @prop)
            args: (argument_list (string) @root_name))
    ]
    (#eq? @obj "rootProject")
    (#eq? @prop "name")
) @root_project_assignment

;; Support multiple arguments in include
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
