mod command;
mod completer;
mod context;
mod handlers;
mod highlighter;
mod prompt;
mod view;

use reedline::{
    default_emacs_keybindings, ColumnarMenu, DefaultHinter, Emacs, FileBackedHistory, KeyCode,
    KeyModifiers, MenuBuilder, Reedline, ReedlineEvent, ReedlineMenu, Signal,
};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tracing::{error, info};

use self::command::{parse_shell_command, ShellCommand};
use self::completer::NaviscopeCompleter;
use self::context::ShellContext;
use self::highlighter::NaviscopeHighlighter;
use self::prompt::DefaultPrompt;

// Shell configuration constants
// Shell configuration constants
const SHELL_HISTORY_SIZE: usize = 500;

pub struct ReplServer {
    context: ShellContext,
    project_path: PathBuf,
    // Runtime must be kept alive for the shell session
    rt: tokio::runtime::Runtime,
}

impl ReplServer {
    pub fn new(project_path: PathBuf) -> Self {
        let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");

        let engine = crate::create_configured_engine(project_path.clone());
        let current_node = Arc::new(RwLock::new(None));

        // Pass runtime handle to context for blocking async calls
        let context = ShellContext::new(engine, rt.handle().clone(), current_node);

        Self {
            context,
            project_path,
            rt,
        }
    }

    pub fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("Project: {:?}", self.project_path);

        self.initialize_index()?;

        // Start watcher (spawns background task on the runtime)
        if let Err(e) = self.rt.block_on(self.context.engine.watch()) {
            error!("Failed to start file watcher: {}", e);
        } else {
            info!("File watcher started.");
        }

        println!("Type 'help' for commands.");

        let line_editor = self.setup_line_editor()?;
        self.run_loop(line_editor)
    }

    fn initialize_index(&self) -> Result<(), Box<dyn std::error::Error>> {
        let engine = &self.context.engine;
        let start = std::time::Instant::now();

        // Load index (blocking on async)
        match self.rt.block_on(engine.load()) {
            Ok(true) => {
                let index = self.context.graph();
                println!(
                    "Index loaded from disk in {:?}. Nodes: {}, Edges: {}",
                    start.elapsed(),
                    index.topology().node_count(),
                    index.topology().edge_count()
                );
            }
            Ok(false) => {
                println!("No existing index found or it was stale. Rebuilding...");
                // If load returns false, we should verify/rebuild.
                // But refresh() below will handle it anyway.
            }
            Err(e) => {
                error!("Failed to load index: {}", e);
                // Continue to rebuild
            }
        }

        // Sync with filesystem (rebuild/refresh)
        let sync_start = std::time::Instant::now();
        if let Err(e) = self.rt.block_on(engine.refresh()) {
            error!("Synchronization failed: {}", e);
            println!("Warning: Index synchronization failed: {}", e);
        } else {
            let index = self.context.graph();
            println!(
                "Index synchronized in {:?}. Total nodes: {}",
                sync_start.elapsed(),
                index.topology().node_count()
            );

            // Auto-set context to Project node if it exists
            use naviscope_core::model::graph::NodeKind;

            let project_nodes: Vec<_> = index
                .topology()
                .node_indices()
                .filter(|&idx| {
                    let node = &index.topology()[idx];
                    node.kind() == NodeKind::Project
                })
                .collect();

            if project_nodes.len() == 1 {
                let fqn = index.topology()[project_nodes[0]].fqn().to_string();
                self.context.set_current_fqn(Some(fqn));
            }
        }
        Ok(())
    }

    // Manual start_watcher removed - handled by EngineHandle::watch()

    fn setup_line_editor(&self) -> Result<Reedline, Box<dyn std::error::Error>> {
        let commands = ShellCommand::command_names();

        let completer = Box::new(NaviscopeCompleter::new(
            commands.clone(),
            self.context.clone(),
        ));

        let completion_menu = Box::new(ColumnarMenu::default().with_name("completion_menu"));

        let mut keybindings = default_emacs_keybindings();
        keybindings.add_binding(
            KeyModifiers::NONE,
            KeyCode::Tab,
            ReedlineEvent::UntilFound(vec![
                ReedlineEvent::Menu("completion_menu".to_string()),
                ReedlineEvent::MenuNext,
            ]),
        );

        let history_file = dirs::home_dir()
            .map(|mut p| {
                p.push(".naviscope");
                p.push("shell");
                let _ = std::fs::create_dir_all(&p);
                p.push("history");
                p
            })
            .unwrap();

        let history = Box::new(
            FileBackedHistory::with_file(SHELL_HISTORY_SIZE, history_file.clone()).unwrap_or_else(
                |_| FileBackedHistory::new(SHELL_HISTORY_SIZE).expect("Failed to create history"),
            ),
        );

        let highlighter = Box::new(NaviscopeHighlighter::new(commands));

        Ok(Reedline::create()
            .with_history(history)
            .with_completer(completer)
            .with_highlighter(highlighter)
            .with_menu(ReedlineMenu::EngineCompleter(completion_menu))
            .with_hinter(Box::new(
                DefaultHinter::default().with_style(
                    nu_ansi_term::Style::new()
                        .italic()
                        .fg(nu_ansi_term::Color::LightGray),
                ),
            ))
            .with_edit_mode(Box::new(Emacs::new(keybindings))))
    }

    fn run_loop(&self, mut line_editor: Reedline) -> Result<(), Box<dyn std::error::Error>> {
        let mut context = self.context.clone();

        loop {
            let curr = context.current_fqn();
            let prompt = DefaultPrompt::new(curr.clone());
            let sig = line_editor.read_line(&prompt);

            match sig {
                Ok(Signal::Success(buffer)) => {
                    let trimmed = buffer.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    if trimmed == "exit" || trimmed == "quit" {
                        break;
                    }

                    match parse_shell_command(trimmed) {
                        Ok(Some(cmd)) => {
                            let handler = self::handlers::get_handler(&cmd);

                            match handler.handle(&cmd, &mut context) {
                                Ok(output) => {
                                    if !output.is_empty() {
                                        println!("{}", output);
                                    }
                                    if matches!(cmd, ShellCommand::Clear) {
                                        let _ = line_editor.clear_screen();
                                    }
                                }
                                Err(e) => eprintln!("Error: {}", e),
                            }
                        }
                        Ok(None) => {} // Help or handled by Clap
                        Err(e) => eprintln!("Error: {}", e),
                    }
                }
                Ok(Signal::CtrlD) | Ok(Signal::CtrlC) => {
                    println!("Bye!");
                    break;
                }
                x => println!("Event: {:?}", x),
            }
        }
        Ok(())
    }
}

pub fn run(path: Option<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    let project_path =
        path.unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let server = ReplServer::new(project_path);
    server.run()
}
