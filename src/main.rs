mod app;
mod cli;
mod layer_shell;
mod note_window;
mod webview_bridge;
mod write_authority;

use app::NoteItApp;
use clap::error::ErrorKind;
use clap::Parser;
use cli::CliArgs;
use gio::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

const APPLICATION_ID: &str = "io.github.theghols.NoteIt";

fn main() -> glib::ExitCode {
    // WebKitGTK's automatically selected Wayland input context can drop dead-key
    // composition on Niri. Respect explicit IME choices, but use GTK's built-in
    // compose context when the environment has not selected one.
    if std::env::var_os("GTK_IM_MODULE").is_none() {
        std::env::set_var("GTK_IM_MODULE", "simple");
    }

    if let Err(error) = CliArgs::try_parse() {
        let exit_code = match error.kind() {
            ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => glib::ExitCode::SUCCESS,
            _ => glib::ExitCode::FAILURE,
        };
        let _ = error.print();
        return exit_code;
    }

    let app = gtk4::Application::builder()
        .application_id(APPLICATION_ID)
        .flags(gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();

    // The primary instance either owns the store or does not start. There is
    // deliberately no third state: a Note-it that is running, editable and not
    // the store's writer would be a second writer, which is the one thing the
    // whole coordination design exists to prevent.
    let note_app: Rc<RefCell<Option<NoteItApp>>> = Rc::new(RefCell::new(None));
    let refused: Rc<Cell<bool>> = Rc::new(Cell::new(false));

    let note_app_startup = Rc::clone(&note_app);
    let refused_startup = Rc::clone(&refused);
    app.connect_startup(move |gtk_app| {
        match NoteItApp::new(gtk_app) {
            Ok(note_app) => *note_app_startup.borrow_mut() = Some(note_app),
            Err(refusal) => {
                // Said once, in a sentence a person can act on, and then the
                // application ends. Nothing has been restored, nothing has been
                // created and nothing is editable, because none of that happens
                // before the store is claimed.
                eprintln!("{refusal}");
                refused_startup.set(true);
                gtk_app.quit();
            }
        }
    });

    let note_app_cmd = Rc::clone(&note_app);
    let refused_cmd = Rc::clone(&refused);
    app.connect_command_line(move |_gtk_app, cmd_line| {
        let args = cmd_line.arguments();
        let args_vec: Vec<String> = args
            .into_iter()
            .map(|os_str| os_str.to_string_lossy().to_string())
            .collect();

        let parsed = match CliArgs::try_parse_from(&args_vec) {
            Ok(parsed) => parsed,
            Err(e) => {
                let _ = e.print();
                return glib::ExitCode::SUCCESS;
            }
        };

        match *note_app_cmd.borrow() {
            Some(ref note_app) => {
                note_app.handle_command(parsed.command, parsed.background);
                glib::ExitCode::SUCCESS
            }
            // No application, so no command. A caller that asked for a new note
            // is told the request did not happen rather than being answered
            // with a success nothing carried out.
            None => {
                refused_cmd.set(true);
                glib::ExitCode::FAILURE
            }
        }
    });

    let exit_code = app.run();
    if refused.get() {
        return glib::ExitCode::FAILURE;
    }
    exit_code
}
