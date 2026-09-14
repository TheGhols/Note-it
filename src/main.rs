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

/// GLib's own flag, and the reason cold D-Bus activation could not work.
///
/// A D-Bus service file starts an application as
/// `note-it --gapplication-service`, and `g_application_run` reads that flag
/// itself. Until now `clap` saw it first, did not recognise it, printed a
/// usage error and returned before GTK was ever built — so the session bus
/// could never start Note-it on demand, and
/// `gapplication action io.github.theghols.NoteIt toggle-layer` worked only
/// while Note-it happened to be running already. That is the global
/// `Ctrl+Shift+Space` binding, and "only when it is already open" is not what
/// a binding is for.
///
/// It is filtered out of *our* parse and left in the real `argv`, which is
/// what `run()` passes to GLib: the flag belongs to GLib and is answered by
/// GLib.
const GAPPLICATION_SERVICE: &str = "--gapplication-service";

/// The arguments this application is meant to parse, without GLib's.
fn arguments_for_clap<I>(arguments: I) -> Vec<std::ffi::OsString>
where
    I: IntoIterator<Item = std::ffi::OsString>,
{
    arguments
        .into_iter()
        .filter(|argument| argument != GAPPLICATION_SERVICE)
        .collect()
}

fn main() -> glib::ExitCode {
    // WebKitGTK's automatically selected Wayland input context can drop dead-key
    // composition on Niri. Respect explicit IME choices, but use GTK's built-in
    // compose context when the environment has not selected one.
    if std::env::var_os("GTK_IM_MODULE").is_none() {
        std::env::set_var("GTK_IM_MODULE", "simple");
    }

    if let Err(error) = CliArgs::try_parse_from(arguments_for_clap(std::env::args_os())) {
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
        // The same filter as above. A client never sends GLib's flag, but a
        // service-mode start hands its own argv here, and one usage error at
        // that moment would be a Note-it that refuses to be activated.
        let args_vec = arguments_for_clap(cmd_line.arguments());

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    fn argv(parts: &[&str]) -> Vec<OsString> {
        parts.iter().map(OsString::from).collect()
    }

    #[test]
    fn glibs_service_flag_is_not_ours_to_parse() {
        // What a D-Bus service file invokes. It must reach `clap` as a bare
        // summon, because the flag is GLib's and `clap` would reject it.
        let filtered = arguments_for_clap(argv(&["note-it", GAPPLICATION_SERVICE]));
        assert_eq!(filtered, argv(&["note-it"]));
        let parsed = CliArgs::try_parse_from(&filtered).expect("a bare summon");
        assert_eq!(parsed.command, None);
        assert!(!parsed.background);
    }

    #[test]
    fn the_service_flag_does_not_swallow_a_command_beside_it() {
        let filtered = arguments_for_clap(argv(&["note-it", GAPPLICATION_SERVICE, "new"]));
        let parsed = CliArgs::try_parse_from(&filtered).expect("a command");
        assert_eq!(parsed.command, Some(cli::CliCommand::New));
    }

    #[test]
    fn every_other_argument_is_left_exactly_as_it_was() {
        // The filter removes one spelling and nothing else: an unknown flag
        // must still be an unknown flag, or this would be a way to smuggle
        // arguments past the parser.
        let untouched = argv(&["note-it", "--background", "toggle", "--nonsense"]);
        assert_eq!(arguments_for_clap(untouched.clone()), untouched);
        assert!(CliArgs::try_parse_from(&untouched).is_err());
    }
}
