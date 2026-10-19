; Nodes the bounded C# scanner handles. Capture names select the handler;
; everything else in the syntax tree is only traversed through.

(variable_declarator) @binding

[
  (compilation_unit)
  (namespace_declaration)
  (file_scoped_namespace_declaration)
] @scope

[
  (class_declaration)
  (record_declaration)
  (struct_declaration)
  (interface_declaration)
  (enum_declaration)
] @class_like

(method_declaration) @method

(constructor_declaration) @constructor

(property_declaration) @property

(invocation_expression) @invocation
