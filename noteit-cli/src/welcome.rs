//! The screen a person sees when they type `noteit` and nothing else.
//!
//! Part of the human renderer — [`crate::output`] enters it and nothing else
//! does. The machine adapter has its own answer for the same outcome (a small
//! JSON document) and never borrows a line from here.
//!
//! Two things vary, and they vary independently:
//!
//! * **Colour** follows [`OutputContext::color_enabled`], which is a question
//!   about the channel: a terminal, no `NO_COLOR`, no `TERM=dumb`.
//! * **Layout** follows the terminal's width, which is a question about the
//!   window. A narrow window gets less art, never a wrapped mess.
//!
//! Everything the screen says survives both: strip the styling and drop to the
//! narrowest layout and the version, the invitation and the commands are still
//! there in plain text. Colour is never the only thing carrying a fact.
//!
//! The screen is a pure function of those two inputs and the package version.
//! It reads nothing, opens nothing and writes nothing — running `noteit` with
//! no arguments cannot create a store, let alone change one.

use crate::output::OutputContext;

/// The wordmark, in the block style the project chose for its terminal
/// identity. Six lines, the widest of them [`LOGO_WIDTH`] columns.
///
/// Every glyph here is a box-drawing or block-element character, all of them
/// single-width, so the width in characters is the width in columns.
const LOGO: &str = "\
███╗   ██╗ ██████╗ ████████╗███████╗      ██╗████████╗
████╗  ██║██╔═══██╗╚══██╔══╝██╔════╝      ██║╚══██╔══╝
██╔██╗ ██║██║   ██║   ██║   █████╗  █████╗██║   ██║
██║╚██╗██║██║   ██║   ██║   ██╔══╝  ╚════╝██║   ██║
██║ ╚████║╚██████╔╝   ██║   ███████╗      ██║   ██║
╚═╝  ╚═══╝ ╚═════╝    ╚═╝   ╚══════╝      ╚═╝   ╚═╝";

/// Columns the block wordmark needs.
pub const LOGO_WIDTH: usize = 54;

/// The wordmark as a word, for the layouts with no room for the art.
const WORDMARK: &str = "NOTE-IT";

/// What Note-it is, at length and in brief. The short one is offered when the
/// long one would wrap, because a wrapped sentence under a logo looks like a
/// bug rather than a subtitle.
const TAGLINE: &str = "Notas rápidas, locais e prontas para você e seus agentes.";
const TAGLINE_SHORT: &str = "Notas rápidas e locais.";

/// The invitation, and the five commands it points at.
const INVITATION: &str = "Comece por:";
const QUICK_COMMANDS: &[&str] = &[
    "noteit listar",
    "noteit buscar \"texto\"",
    "noteit criar \"Minha nota\"",
    "noteit status",
    "noteit ajuda",
];

/// The two a person needs when there is room for nothing else: one that shows
/// them their notes, one that shows them everything else.
const ESSENTIAL_COMMANDS: &[&str] = &["noteit listar", "noteit ajuda"];

/// Every command line is indented by this much, which counts towards its width.
const INDENT: &str = "  ";

/// How much of the screen fits.
///
/// A value rather than a chain of comparisons at the point of use, so the
/// decision can be tested on its own and the renderer never has to re-derive
/// it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// The block wordmark and the full invitation.
    Full,
    /// No art: the wordmark as a word, and the same five commands.
    Compact,
    /// The wordmark, the version and the two commands that matter most.
    Minimal,
}

/// The narrowest window the compact layout is still honest in — the width of
/// its longest line, `  noteit criar "Minha nota"`.
const COMPACT_MIN_WIDTH: usize = 27;

/// Which layout a terminal of this many columns gets.
pub fn layout_for(width: usize) -> Layout {
    if width >= LOGO_WIDTH {
        Layout::Full
    } else if width >= COMPACT_MIN_WIDTH {
        Layout::Compact
    } else {
        Layout::Minimal
    }
}

/// The whole screen, as the text to be written to standard output.
pub fn render(ctx: &OutputContext) -> String {
    let width = ctx.effective_width();
    let layout = match layout_for(width) {
        // Room for the art is not permission to draw it: a terminal that
        // announced itself as `dumb` gets the conservative screen at any size.
        Layout::Full if !ctx.block_art_enabled => Layout::Compact,
        chosen => chosen,
    };
    match layout {
        Layout::Full => render_full(ctx, width),
        Layout::Compact => render_compact(ctx, width),
        Layout::Minimal => render_minimal(ctx),
    }
}

/// `Note-it 0.1.0`, from the package's own version and no other source, so it
/// can never disagree with `noteit versao`.
fn version_line(ctx: &OutputContext) -> String {
    format!(
        "{} {}",
        ctx.bold("Note-it"),
        ctx.magenta(env!("CARGO_PKG_VERSION"))
    )
}

/// `NOTE-IT 0.1.0`, the same fact wearing the wordmark, for the layouts with
/// no room to say it twice.
fn wordmark_line(ctx: &OutputContext) -> String {
    format!(
        "{} {}",
        ctx.yellow(WORDMARK),
        ctx.magenta(env!("CARGO_PKG_VERSION"))
    )
}

/// The longest tagline this many columns can hold without wrapping, if any.
fn tagline_for(width: usize) -> Option<&'static str> {
    if width >= TAGLINE.chars().count() {
        Some(TAGLINE)
    } else if width >= TAGLINE_SHORT.chars().count() {
        Some(TAGLINE_SHORT)
    } else {
        None
    }
}

fn push_commands(out: &mut String, commands: &[&str]) {
    for command in commands {
        out.push_str(INDENT);
        out.push_str(command);
        out.push('\n');
    }
}

fn render_full(ctx: &OutputContext, width: usize) -> String {
    let mut out = String::new();
    for line in LOGO.lines() {
        out.push_str(&ctx.yellow(line));
        out.push('\n');
    }
    out.push('\n');
    out.push_str(&version_line(ctx));
    out.push('\n');
    if let Some(tagline) = tagline_for(width) {
        out.push_str(&ctx.dim(tagline));
        out.push('\n');
    }
    out.push('\n');
    out.push_str(&ctx.magenta(INVITATION));
    out.push('\n');
    push_commands(&mut out, QUICK_COMMANDS);
    out
}

fn render_compact(ctx: &OutputContext, width: usize) -> String {
    let mut out = String::new();
    out.push_str(&wordmark_line(ctx));
    out.push('\n');
    if let Some(tagline) = tagline_for(width) {
        out.push_str(&ctx.dim(tagline));
        out.push('\n');
    }
    out.push('\n');
    out.push_str(&ctx.magenta(INVITATION));
    out.push('\n');
    push_commands(&mut out, QUICK_COMMANDS);
    out
}

fn render_minimal(ctx: &OutputContext) -> String {
    let mut out = String::new();
    out.push_str(&wordmark_line(ctx));
    out.push('\n');
    out.push('\n');
    push_commands(&mut out, ESSENTIAL_COMMANDS);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The screen with every escape removed, which is what a person reads
    /// through the styling and what a pipe receives instead of it.
    fn plain_at(width: usize) -> String {
        render(&OutputContext::plain().with_width(Some(width)))
    }

    fn longest_line(text: &str) -> usize {
        text.lines()
            .map(|line| line.chars().count())
            .max()
            .unwrap_or(0)
    }

    #[test]
    fn the_stated_logo_width_is_the_width_the_logo_actually_has() {
        assert_eq!(longest_line(LOGO), LOGO_WIDTH);
        assert_eq!(LOGO.lines().count(), 6);
    }

    #[test]
    fn the_compact_threshold_is_the_width_of_its_longest_line() {
        let longest = QUICK_COMMANDS
            .iter()
            .map(|command| INDENT.chars().count() + command.chars().count())
            .max()
            .expect("there is at least one quick command");
        assert_eq!(longest, COMPACT_MIN_WIDTH);
    }

    #[test]
    fn the_layout_tiers_are_decided_by_width_alone() {
        assert_eq!(layout_for(120), Layout::Full);
        assert_eq!(layout_for(LOGO_WIDTH), Layout::Full);
        assert_eq!(layout_for(LOGO_WIDTH - 1), Layout::Compact);
        assert_eq!(layout_for(COMPACT_MIN_WIDTH), Layout::Compact);
        assert_eq!(layout_for(COMPACT_MIN_WIDTH - 1), Layout::Minimal);
        assert_eq!(layout_for(1), Layout::Minimal);
    }

    #[test]
    fn no_layout_is_ever_wider_than_the_window_it_was_drawn_for() {
        for width in 1..=200usize {
            let screen = plain_at(width);
            // Below the minimal layout's own longest line there is nothing
            // left to cut: `  noteit listar` is the floor, and a window
            // narrower than that has no honest rendering.
            let floor = INDENT.chars().count()
                + ESSENTIAL_COMMANDS
                    .iter()
                    .map(|command| command.chars().count())
                    .max()
                    .expect("there is at least one essential command");
            let allowed = width.max(floor);
            assert!(
                longest_line(&screen) <= allowed,
                "width {width}: longest line is {} columns",
                longest_line(&screen)
            );
        }
    }

    #[test]
    fn every_layout_names_the_version_and_at_least_one_command() {
        for width in [200, 80, LOGO_WIDTH, 40, COMPACT_MIN_WIDTH, 20, 1] {
            let screen = plain_at(width);
            assert!(
                screen.contains(env!("CARGO_PKG_VERSION")),
                "width {width} lost the version"
            );
            assert!(
                screen.contains("noteit listar"),
                "width {width} lost every command"
            );
            assert!(
                screen.contains("noteit ajuda"),
                "width {width} lost the way to the help"
            );
        }
    }

    #[test]
    fn the_wide_layout_draws_the_wordmark_and_the_narrow_ones_spell_it() {
        let wide = plain_at(100);
        assert!(wide.starts_with("███╗"), "the block wordmark is missing");
        assert!(wide.contains(TAGLINE));
        assert!(wide.contains(INVITATION));

        let compact = plain_at(40);
        assert!(!compact.contains('█'), "the art survived a narrow window");
        assert!(compact.starts_with("NOTE-IT "));
        assert!(compact.contains(TAGLINE_SHORT));
        assert!(compact.contains(INVITATION));

        let minimal = plain_at(20);
        assert!(!minimal.contains('█'));
        assert!(minimal.starts_with("NOTE-IT "));
        assert!(!minimal.contains(TAGLINE_SHORT));
    }

    #[test]
    fn the_tagline_is_chosen_by_what_fits_and_dropped_when_nothing_does() {
        assert_eq!(tagline_for(200), Some(TAGLINE));
        assert_eq!(tagline_for(TAGLINE.chars().count()), Some(TAGLINE));
        assert_eq!(
            tagline_for(TAGLINE.chars().count() - 1),
            Some(TAGLINE_SHORT)
        );
        assert_eq!(
            tagline_for(TAGLINE_SHORT.chars().count()),
            Some(TAGLINE_SHORT)
        );
        assert_eq!(tagline_for(TAGLINE_SHORT.chars().count() - 1), None);
    }

    #[test]
    fn styling_adds_colour_and_removes_nothing() {
        for width in [100, 40, 20] {
            let styled = render(&OutputContext::styled().with_width(Some(width)));
            let plain = plain_at(width);
            assert!(styled.contains('\u{1b}'), "width {width} was never styled");
            assert_eq!(
                crate::output::sanitize_for_terminal(&styled),
                plain,
                "width {width}: styling changed what the screen says"
            );
        }
    }

    #[test]
    fn the_brand_uses_yellow_and_magenta_and_nothing_else() {
        let styled = render(&OutputContext::styled().with_width(Some(100)));
        // 33 is the yellow of the wordmark, 35 the magenta accent; 1 and 2 are
        // weight, not hue. Any other colour would be a fourth voice on a
        // screen that was asked to have two.
        for code in ["\u{1b}[31m", "\u{1b}[32m", "\u{1b}[34m", "\u{1b}[36m"] {
            assert!(!styled.contains(code), "{code:?} is not a brand colour");
        }
        assert!(styled.contains("\u{1b}[33m"), "the yellow is missing");
        assert!(styled.contains("\u{1b}[35m"), "the magenta is missing");
    }

    #[test]
    fn a_dumb_terminal_gets_the_conservative_screen_however_wide_it_is() {
        for width in [200, 100, LOGO_WIDTH] {
            let dumb = render(
                &OutputContext::plain()
                    .with_width(Some(width))
                    .with_block_art(false),
            );
            assert!(
                !dumb.contains('█'),
                "width {width}: a dumb terminal was handed block art"
            );
            // It loses the art and nothing else: the same compact screen a
            // narrow window gets, laid out for the width it really has.
            assert_eq!(
                dumb,
                render_compact(&OutputContext::plain(), width),
                "width {width}: a dumb terminal got something other than the compact screen"
            );
        }

        // Below the art's width the two answers were already the same, so the
        // capability changes nothing there.
        for width in [40, 20] {
            assert_eq!(
                render(
                    &OutputContext::plain()
                        .with_width(Some(width))
                        .with_block_art(false)
                ),
                plain_at(width)
            );
        }
    }

    #[test]
    fn an_unmeasured_terminal_gets_the_full_screen() {
        // Nothing answered, so the conservative assumption applies — and it is
        // wide enough for the whole thing.
        assert_eq!(
            render(&OutputContext::plain()),
            plain_at(OutputContext::ASSUMED_WIDTH)
        );
        assert_eq!(layout_for(OutputContext::ASSUMED_WIDTH), Layout::Full);
    }
}
