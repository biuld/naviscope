use naviscope::query::QueryEngine;
use naviscope::query::GraphQuery;
use super::command::ShellCommand;
use super::context::{ShellContext, ResolveResult};

pub trait CommandHandler {
    fn handle(&self, cmd: &ShellCommand, context: &mut ShellContext) -> Result<String, Box<dyn std::error::Error>>;
}

pub struct CdHandler;
impl CommandHandler for CdHandler {
    fn handle(&self, cmd: &ShellCommand, context: &mut ShellContext) -> Result<String, Box<dyn std::error::Error>> {
        if let ShellCommand::Cd { path } = cmd {
            match context.resolve_node(path) {
                ResolveResult::Found(fqn) => {
                    let new_curr = if fqn.is_empty() { None } else { Some(fqn) };
                    context.set_current_fqn(new_curr);
                    Ok(String::new())
                },
                ResolveResult::Ambiguous(candidates) => {
                    let mut msg = format!("Ambiguous path '{}'. Candidates:\n", path);
                    for c in candidates.iter().take(10) {
                        msg.push_str(&format!("  - {}\n", c));
                    }
                    Err(msg.into())
                },
                ResolveResult::NotFound => Err(format!("Node '{}' not found.", path).into()),
            }
        } else {
            Ok(String::new())
        }
    }
}

pub struct CatHandler;
impl CommandHandler for CatHandler {
    fn handle(&self, cmd: &ShellCommand, context: &mut ShellContext) -> Result<String, Box<dyn std::error::Error>> {
        if let ShellCommand::Cat { target } = cmd {
            // First resolve the target to a concrete FQN
            let fqn = match context.resolve_node(target) {
                ResolveResult::Found(f) => f,
                ResolveResult::Ambiguous(candidates) => {
                    let mut msg = format!("Ambiguous match for '{}'. Available options:\n\n", target);
                    // We should probably look up the names for these FQNs to show better hints, 
                    // but for now showing FQNs is correct.
                    for c in candidates {
                        msg.push_str(&format!("  - {}\n", c));
                    }
                    msg.push_str("\nPlease specify the full name.");
                    return Ok(msg);
                },
                ResolveResult::NotFound => {
                    // If not resolved locally, fallback to trying target as raw FQN (handled by engine)
                    // This covers cases where target is an FQN but not reachable via 'ls' from current node?
                    // Actually resolve_node step 3 covers Exact Match. So it's truly not found.
                    return Ok("NO RECORDS FOUND".to_string());
                }
            };

            if fqn.is_empty() {
                 return Err("Cannot cat root.".into());
            }

            let engine_guard = context.naviscope.read().unwrap();
            let engine = QueryEngine::new(engine_guard.graph());
            let query = GraphQuery::Cat { fqn };
            let result = engine.execute(&query)?;
            
            // Re-use ShellCommand's render for consistent output format
            cmd.render(result)
        } else {
            Ok(String::new())
        }
    }
}

pub struct GenericQueryHandler;
impl CommandHandler for GenericQueryHandler {
    fn handle(&self, cmd: &ShellCommand, context: &mut ShellContext) -> Result<String, Box<dyn std::error::Error>> {
        let current_node = context.current_fqn();
        let query = cmd.to_graph_query(&current_node)?;
        
        let engine_guard = context.naviscope.read().unwrap();
        let engine = QueryEngine::new(engine_guard.graph());
        
        let result = engine.execute(&query)?;
        cmd.render(result)
    }
}

pub struct PwdHandler;
impl CommandHandler for PwdHandler {
    fn handle(&self, _cmd: &ShellCommand, context: &mut ShellContext) -> Result<String, Box<dyn std::error::Error>> {
        Ok(context.current_fqn().unwrap_or("/".to_string()))
    }
}

pub struct ClearHandler;
impl CommandHandler for ClearHandler {
    fn handle(&self, _cmd: &ShellCommand, _context: &mut ShellContext) -> Result<String, Box<dyn std::error::Error>> {
        // Clear is handled by the Reedline loop mostly, but we can return a marker if needed.
        // For now, simple print or empty string. The loop handles `line_editor.clear_screen()`.
        Ok(String::new())
    }
}

pub fn get_handler(cmd: &ShellCommand) -> Box<dyn CommandHandler> {
    match cmd {
        ShellCommand::Cd { .. } => Box::new(CdHandler),
        ShellCommand::Cat { .. } => Box::new(CatHandler),
        ShellCommand::Pwd => Box::new(PwdHandler),
        ShellCommand::Clear => Box::new(ClearHandler),
        _ => Box::new(GenericQueryHandler),
    }
}
