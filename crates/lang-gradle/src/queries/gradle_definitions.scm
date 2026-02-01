;; Pattern for the dependencies block
(
    [
        (method_invocation name: (identifier) @name)
        (juxt_function_call name: (identifier) @name)
    ]
    (#eq? @name "dependencies")
) @dependencies_block

;; Pattern for dependency items (External libraries)
(
    [
        (method_invocation
            name: (identifier) @method_name
            arguments: (argument_list [ (string_literal) (character_literal) ] @dep_string))
        (juxt_function_call
            name: (identifier) @method_name
            args: (argument_list [ (string_literal) (character_literal) ] @dep_string))
    ]
    (#match? @method_name "^(implementation|api|testImplementation|compileOnly|runtimeOnly|annotationProcessor)$")
) @dependency_item

;; Pattern for project dependencies (Parentheses)
(method_invocation
    name: (identifier) @method_name
    arguments: (argument_list
        (method_invocation
            name: (identifier) @proj_fn
            arguments: (argument_list [ (string_literal) (character_literal) ] @project_path)))) @project_dependency_item
(#match? @method_name "^(implementation|api|testImplementation|compileOnly|runtimeOnly|annotationProcessor)$")
(#eq? @proj_fn "project")

;; Pattern for project dependencies (Juxt + Sibling in closure)
(
    (juxt_function_call
        name: (identifier) @method_name
        args: (argument_list (identifier) @proj_fn)) @project_dependency_item
    .
    (expression_statement (parenthesized_expression [ (string_literal) (character_literal) ] @project_path))
    (#match? @method_name "^(implementation|api|testImplementation|compileOnly|runtimeOnly|annotationProcessor)$")
    (#eq? @proj_fn "project")
)

;; Pattern for settings.gradle: rootProject.name = '...'
(
    [
        (assignment_expression
            left: (field_access
                object: (identifier) @obj
                field: (identifier) @prop)
            right: [ (string_literal) (character_literal) ] @root_name)
        (method_invocation
            object: (identifier) @obj
            name: (identifier) @prop
            arguments: (argument_list [ (string_literal) (character_literal) ] @root_name))
    ]
    (#eq? @obj "rootProject")
    (#eq? @prop "name")
) @root_project_assignment

;; Support multiple arguments in include
(
    [
        (method_invocation
            name: (identifier) @include_fn
            arguments: (argument_list [ (string_literal) (character_literal) ] @included_path))
        (juxt_function_call
            name: (identifier) @include_fn
            args: (argument_list [ (string_literal) (character_literal) ] @included_path))
    ]
    (#eq? @include_fn "include")
) @include_call
