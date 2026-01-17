pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    println!("GraphQuery JSON Interface Specification:");
    println!("======================================");
    
    println!("\nAvailable Node Kinds (for kind filters):");
    println!("  - Java: class, interface, enum, annotation, method, field");
    println!("  - Build: package (module), dependency");

    println!("\nAvailable Edge Types (for edge_type filters):");
    println!("  Contains, InheritsFrom, Implements, Calls, References, Instantiates, UsesDependency");

    println!("\n1. GREP - Search for symbols");
    let grep = serde_json::json!({
        "command": "grep",
        "pattern": "String (Required) - Regex or plain text",
        "kind": "Array of Strings (Optional, Default: []) - e.g., ['class', 'method']",
        "limit": "Number (Optional, Default: 20)"
    });
    println!("{}", serde_json::to_string_pretty(&grep)?);

    println!("\n2. LS - List members of a node or project");
    let ls = serde_json::json!({
        "command": "ls",
        "fqn": "String (Optional, Default: null) - Full path of node to list",
        "kind": "Array of Strings (Optional, Default: []) - Filter by node kind"
    });
    println!("{}", serde_json::to_string_pretty(&ls)?);

    println!("\n3. INSPECT - Get full details of a node");
    let inspect = serde_json::json!({
        "command": "inspect",
        "fqn": "String (Required) - Target node FQN"
    });
    println!("{}", serde_json::to_string_pretty(&inspect)?);

    println!("\n4. INCOMING / OUTGOING - Trace relationships");
    let relations = serde_json::json!({
        "command": "incoming | outgoing",
        "fqn": "String (Required) - Target node FQN",
        "edge_type": "Array of EdgeTypes (Optional, Default: []) - Filter by relationship type"
    });
    println!("{}", serde_json::to_string_pretty(&relations)?);

    Ok(())
}
