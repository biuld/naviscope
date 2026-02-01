;; Intent-aware SCM queries for finding symbol occurrences

;; Method Intent
[
  (method_invocation name: (identifier) @ident)
  (method_declaration name: (identifier) @ident)
  (constructor_declaration name: (identifier) @ident)
] @method_occurrence

;; Type Intent
(type_identifier) @ident @type_occurrence

;; Field Intent
[
  (field_access field: (identifier) @ident)
  (variable_declarator name: (identifier) @ident)
] @field_occurrence

;; Generic Fallback
[
  (identifier) @ident
  (type_identifier) @ident
] @generic_occurrence
