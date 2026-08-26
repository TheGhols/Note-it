mod app;
mod cli;
mod layer_shell;
mod model;
mod note_window;
mod settings;
mod state;
mod storage;
mod webview_bridge;

use app::NoteItApp;
use clap::Parser;
use cli::CliArgs;
use gio::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

const APPLICATION_ID: &str = "io.github.theghols.NoteIt";

fn main() -> glib::ExitCode {
    let app = gtk4::Application::builder()
        .application_id(APPLICATION_ID)
        .flags(gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();

    let note_app: Rc<RefCell<Option<NoteItApp>>> = Rc::new(RefCell::new(None));

    let note_app_startup = Rc::clone(&note_app);
    app.connect_startup(move |gtk_app| {
        *note_app_startup.borrow_mut() = Some(NoteItApp::new(gtk_app));
    });

    let note_app_cmd = Rc::clone(&note_app);
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

        if let Some(ref note_app) = *note_app_cmd.borrow() {
            note_app.handle_command(parsed.command, parsed.background);
        }

        glib::ExitCode::SUCCESS
    });

    app.run()
}
