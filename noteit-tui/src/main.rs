use noteit_core::StorePaths;
use noteit_tui::app::App;
use noteit_tui::terminal;
use std::env;
use std::io::{self, IsTerminal};
use std::process;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn print_help() {
    println!(
        "Note-it TUI {VERSION}\n\
Interface de terminal interativa para o Note-it\n\n\
USO:\n    \
noteit-tui [OPÇÕES]\n\n\
OPÇÕES:\n    \
-h, --help       Exibe esta mensagem de ajuda\n    \
-v, --version    Exibe a versão do programa\n"
    );
}

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();

    // 1. Command-line flags (help, version)
    if args
        .iter()
        .any(|arg| arg == "-h" || arg == "--help" || arg == "ajuda")
    {
        print_help();
        process::exit(0);
    }
    if args
        .iter()
        .any(|arg| arg == "-v" || arg == "--version" || arg == "versao")
    {
        println!("noteit-tui {VERSION}");
        process::exit(0);
    }

    // 2. Preflight check: interactive terminal
    if !io::stdout().is_terminal() {
        eprintln!("erro: noteit-tui requer um terminal interativo (tty).");
        process::exit(1);
    }

    // 3. Preflight check: Note-it store configuration
    let paths = StorePaths::resolve();
    if !paths.notes_dir.exists() {
        eprintln!(
            "erro: store do Note-it não encontrado em {}.\n\
Dica: execute 'note-it' para inicializar o aplicativo ou crie uma nota com 'noteit adicionar'.",
            paths.notes_dir.display()
        );
        process::exit(1);
    }

    // 4. Install panic hook before any terminal state alteration
    terminal::install_panic_hook();

    // 5. Install signal handlers (SIGINT, SIGTERM)
    let term_flag = Arc::new(AtomicBool::new(false));
    if let Err(err) = terminal::install_signal_handlers(Arc::clone(&term_flag)) {
        eprintln!("erro ao registrar tratadores de sinais: {err}");
        process::exit(1);
    }

    // 6. Enter raw mode and alternate screen
    let (mut guard, mut terminal) = terminal::TerminalGuard::new()?;

    // 7. Controlled panic hook for automated verification
    if env::var("NOTEIT_TUI_PANIC_FOR_TEST").is_ok()
        || args.iter().any(|arg| arg == "--panic-for-test")
    {
        panic!("pânico controlado para teste de restauração do terminal");
    }

    // 8. Run interactive application event loop
    let mut app = App::new_at(paths, term_flag);
    let run_result = app.run(&mut terminal);

    // 9. Restore terminal safely upon return
    guard.restore()?;

    if let Err(err) = run_result {
        eprintln!("erro durante execução da TUI: {err}");
        process::exit(1);
    }

    Ok(())
}
