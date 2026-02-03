naviscope_plugin::decl_indices!(OccurrenceIndices, {
    ident => "ident",
    method => "method_occurrence",
    type_alias => "type_occurrence",
    field => "field_occurrence",
    generic => "generic_occurrence",
});

pub const JAVA_OCCURRENCES_SCM: &str = include_str!("java_occurrences.scm");
