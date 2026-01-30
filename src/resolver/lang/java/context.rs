use crate::model::graph::ResolvedUnit;
use crate::parser::SymbolIntent;
use crate::parser::java::JavaParser;
use crate::query::CodeGraphLike; // Updated
use tree_sitter::{Node, Tree};

pub struct ResolutionContext<'a> {
    pub node: Node<'a>,
    pub name: String,
    pub index: &'a dyn CodeGraphLike, // Updated type
    pub unit: Option<&'a ResolvedUnit>,
    pub source: &'a str,
    pub tree: &'a Tree,
    pub intent: SymbolIntent,
    pub package: Option<String>,
    pub imports: Vec<String>,
    pub enclosing_classes: Vec<String>,
    pub receiver_node: Option<Node<'a>>,
}

impl<'a> ResolutionContext<'a> {
    pub fn new(
        node: Node<'a>,
        name: String,
        index: &'a dyn CodeGraphLike, // Updated type
        source: &'a str,
        tree: &'a Tree,
        parser: &JavaParser,
    ) -> Self {
        Self::new_with_unit(node, name, index, None, source, tree, parser)
    }

    pub fn new_with_unit(
        node: Node<'a>,
        name: String,
        index: &'a dyn CodeGraphLike, // Updated type
        unit: Option<&'a ResolvedUnit>,
        source: &'a str,
        tree: &'a Tree,
        parser: &JavaParser,
    ) -> Self {
        let (package, imports) = parser.extract_package_and_imports(tree, source);
        let enclosing_classes = parser.get_enclosing_class_fqns(&node, source, package.as_deref());
        let intent = parser.determine_intent(&node);

        let receiver_node = node.parent().and_then(|parent| match parent.kind() {
            "field_access" | "method_invocation" => parent
                .child_by_field_name("object")
                .filter(|obj| obj.id() != node.id()),
            "scoped_type_identifier" => parent
                .child_by_field_name("scope")
                .or_else(|| parent.named_child(0))
                .filter(|obj| obj.id() != node.id()),
            "scoped_identifier" => parent
                .child_by_field_name("scope")
                .filter(|obj| obj.id() != node.id()),
            _ => None,
        });

        Self {
            node,
            name,
            index,
            unit,
            source,
            tree,
            intent,
            package,
            imports,
            enclosing_classes,
            receiver_node,
        }
    }
}
