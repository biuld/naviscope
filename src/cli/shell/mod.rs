mod command;
mod completer;
mod context;
mod handlers;
mod prompt;
mod view;

use naviscope::index::Naviscope;
use naviscope::project::watcher::Watcher;
use reedline::{
    ColumnarMenu, DefaultHinter, Emacs, FileBackedHistory, KeyCode, KeyModifiers, MenuBuilder,
    Reedline, ReedlineEvent, ReedlineMenu, Signal, default_emacs_keybindings,
};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;
use tracing::{error, info};

use self::command::{ShellCommand, parse_shell_command};
use self::completer::NaviscopeCompleter;
use self::context::ShellContext;
use self::prompt::DefaultPrompt;

pub struct ReplServer {
    context: ShellContext,
    project_path: PathBuf,
}

impl ReplServer {
    pub fn new(project_path: PathBuf) -> Self {
        let engine = Naviscope::new(project_path.clone());
        let naviscope = Arc::new(RwLock::new(engine));
        let current_node = Arc::new(RwLock::new(None));
        let context = ShellContext::new(naviscope, current_node);

        Self {
            context,
            project_path,
        }
    }

    pub fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("Project: {:?}", self.project_path);

        self.initialize_index()?;
        self.start_watcher();

        println!("Type 'help' for commands.");

        let line_editor = self.setup_line_editor()?;
        self.run_loop(line_editor)
    }

    fn initialize_index(&self) -> Result<(), Box<dyn std::error::Error>> {
        let mut engine = self.context.naviscope.write().unwrap();
        let start = std::time::Instant::now();

        // Try to load existing index
        match engine.load() {
            Ok(true) => {
                let index = engine.graph();
                println!(
                    "Index loaded from disk in {:?}. Nodes: {}, Edges: {}",
                    start.elapsed(),
                    index.topology.node_count(),
                    index.topology.edge_count()
                );
            }
            Ok(false) => {
                println!("No existing index found. Building fresh index...");
            }
            Err(e) => {
                error!("Failed to load index: {}", e);
                println!("Failed to load index: {}. Starting fresh scan...", e);
            }
        }

        // Sync with filesystem (refresh) synchronously before starting the shell
        let sync_start = std::time::Instant::now();
        if let Err(e) = engine.refresh() {
            error!("Synchronization failed: {}", e);
            println!("Warning: Index synchronization failed: {}", e);
        } else {
            let index = engine.graph();
            println!(
                "Index synchronized in {:?}. Total nodes: {}",
                sync_start.elapsed(),
                index.topology.node_count()
            );
        }
        Ok(())
    }

    fn start_watcher(&self) {
        let naviscope_clone = self.context.naviscope.clone();
        let path_clone = self.project_path.clone();

        thread::spawn(move || {
            let mut watcher = match Watcher::new(&path_clone) {
                Ok(w) => w,
                Err(e) => {
                    error!("Failed to start watcher: {}", e);
                    return;
                }
            };

            loop {
                if let Some(event) = watcher.next_event() {
                    if !event
                        .paths
                        .iter()
                        .any(|p| naviscope::project::is_relevant_path(p))
                    {
                        continue;
                    }

                    thread::sleep(Duration::from_millis(500));
                    while watcher.try_next_event().is_some() {}

                    info!("Change detected. Re-indexing...");

                    match naviscope_clone.write() {
                        Ok(mut engine) => {
                            if let Err(e) = engine.refresh() {
                                error!("Error during re-indexing: {}", e);
                            } else {
                                let index = engine.graph();
                                info!(
                                    "Indexing complete! Nodes: {}, Edges: {}",
                                    index.topology.node_count(),
                                    index.topology.edge_count()
                                );
                            }
                        }
                        Err(e) => error!("Failed to acquire lock for re-indexing: {}", e),
                    }
                }
            }
        });
    }

    fn setup_line_editor(&self) -> Result<Reedline, Box<dyn std::error::Error>> {
        let commands = vec![
            "help".into(),
            "exit".into(),
            "quit".into(),
            "ls".into(),
            "cd".into(),
            "pwd".into(),
            "clear".into(),
            "grep".into(),
            "cat".into(),
            "deps".into(),
        ];

        let completer = Box::new(NaviscopeCompleter::new(commands, self.context.clone()));

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
            FileBackedHistory::with_file(500, history_file.clone())
                .unwrap_or_else(|_| FileBackedHistory::new(500).expect("Failed to create history")),
        );

        Ok(Reedline::create()
            .with_history(history)
            .with_completer(completer)
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
