(package_declaration
  [(identifier) (scoped_identifier)] @package_name)

(import_declaration
  [(identifier) (scoped_identifier)] @import_name)

(class_declaration
  name: (identifier) @class_name) @class_def

(interface_declaration
  name: (identifier) @interface_name) @interface_def

(enum_declaration
  name: (identifier) @enum_name) @enum_def

(annotation_type_declaration
  name: (identifier) @annotation_name) @annotation_def

(method_declaration
  name: (identifier) @method_name) @method_def

(constructor_declaration
  name: (identifier) @constructor_name) @constructor_def

(field_declaration
  declarator: (variable_declarator
    name: (identifier) @field_name)) @field_def

;; Enum constants
(enum_constant
  name: (identifier) @enum_constant)

;; Separate metadata matches to avoid breaking definitions
(class_declaration
  superclass: (superclass) @class_superclass)

(class_declaration
  interfaces: (super_interfaces (type_list (_) @class_interface)))

(interface_declaration
  (extends_interfaces (type_list (_) @interface_extends)))

(enum_declaration
  interfaces: (super_interfaces (type_list (_) @enum_interface)))

(method_declaration
  type: (_) @method_return_type)

(field_declaration
  type: (_) @field_type)

(modifiers) @modifiers

(formal_parameter
  type: (_) @param_type
  name: (identifier) @param_name) @param_match

;; Call and Instantiation
(method_invocation
  name: (identifier) @call_name) @method_call

(object_creation_expression
  type: [ (type_identifier) (generic_type) (scoped_type_identifier) ] @inst_type
) @instantiation
