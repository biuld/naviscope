(package_declaration
  [(identifier) (scoped_identifier)] @package_name)

(import_declaration
  [(identifier) (scoped_identifier)] @import_name)

(class_declaration
  (modifiers ("public" @modifiers)?)
  name: (identifier) @class_name
  superclass: (superclass [ (type_identifier) (generic_type) (scoped_type_identifier) ] @class_superclass)?
  interfaces: (super_interfaces (type_list [ (type_identifier) (generic_type) (scoped_type_identifier) ] @class_interface))?
) @class_def

(interface_declaration
  (modifiers ("public" @modifiers)?)
  name: (identifier) @interface_name
  (extends_interfaces (type_list [ (type_identifier) (generic_type) (scoped_type_identifier) ] @interface_extends))?
) @interface_def

(method_declaration
  (modifiers ("public" @modifiers)?)
  type: [ (type_identifier) (generic_type) (scoped_type_identifier) (void_type) (integral_type) (floating_point_type) (boolean_type) ] @method_return_type
  name: (identifier) @method_name
) @method_def

(method_declaration
  name: (identifier) @method_name
  parameters: (formal_parameters
    (formal_parameter
      type: [ (type_identifier) (generic_type) (scoped_type_identifier) (integral_type) (floating_point_type) (boolean_type) ] @param_type
      name: (identifier) @param_name))
) @method_param_match

(constructor_declaration
  (modifiers ("public" @modifiers)?)
  name: (identifier) @constructor_name
) @constructor_def

(constructor_declaration
  name: (identifier) @constructor_name
  parameters: (formal_parameters
    (formal_parameter
      type: [ (type_identifier) (generic_type) (scoped_type_identifier) (integral_type) (floating_point_type) (boolean_type) ] @param_type
      name: (identifier) @param_name))
) @constructor_param_match

(field_declaration
  (modifiers ("private" @modifiers)?)
  type: [ (type_identifier) (generic_type) (scoped_type_identifier) (integral_type) (floating_point_type) (boolean_type) ] @field_type
  declarator: (variable_declarator
    name: (identifier) @field_name)) @field_def

(method_invocation
  name: (identifier) @call_name) @method_call

(object_creation_expression
  type: [ (type_identifier) (generic_type) ] @inst_type
) @instantiation
