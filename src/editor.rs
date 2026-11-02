//! UI-independent Hydrogen lexical support and lightweight completion.
//! This matches the syntax used by the Hydrogen compiler samples; semantic
//! symbols and compiler diagnostics will eventually come from the LSP.
use eframe::egui::{self, Color32};

// The legacy renderer reuses the same frontend-neutral token model as the
// native GTK/AppKit/WinUI application. Avoid a second lexer implementation.
pub use crate::hydrogen_lexer::{
    completion_candidates, completion_context, insert_completion, prefix_at, tokenize, TokenKind,
};
fn highlight_with_palette(
    text: &str,
    plain: Color32,
    palette: &crate::theme::SyntaxPalette,
) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let tokens = tokenize(text);
    let mut after_declaration = false;
    for (idx, &(kind, content)) in tokens.iter().enumerate() {
        let significant_next = tokens[idx + 1..]
            .iter()
            .find(|(_, s)| !s.trim().is_empty())
            .map(|(_, s)| *s);
        let declaration = matches!(
            content,
            "class" | "struct" | "interface" | "enum" | "namespace"
        );
        let declared_name = after_declaration && kind == TokenKind::Plain;
        let function_name = kind == TokenKind::Plain
            && significant_next == Some("(")
            && !matches!(content, "if" | "while" | "for" | "switch" | "catch");
        let color = if declared_name {
            palette.declaration.color()
        } else if function_name {
            palette.function.color()
        } else {
            match kind {
                TokenKind::Plain => plain,
                TokenKind::Keyword => palette.keyword.color(),
                TokenKind::Modifier => palette.modifier.color(),
                TokenKind::Type => palette.type_name.color(),
                TokenKind::Literal => palette.literal.color(),
                TokenKind::Comment => palette.comment.color(),
                TokenKind::String => palette.string.color(),
                TokenKind::Number => palette.number.color(),
                TokenKind::Operator => palette.operator.color(),
                TokenKind::Punctuation => palette.punctuation.color(),
            }
        };
        job.append(
            content,
            0.0,
            egui::TextFormat {
                font_id: egui::FontId::monospace(14.0),
                color,
                ..Default::default()
            },
        );
        if declaration {
            after_declaration = true;
        } else if after_declaration
            && (declared_name || !content.trim().is_empty() && content != ".")
        {
            after_declaration = false;
        }
    }
    job
}

#[derive(Default)]
struct Highlighter;
impl<'a>
    egui::util::cache::ComputerMut<
        (&'a str, Color32, crate::theme::SyntaxPalette),
        egui::text::LayoutJob,
    > for Highlighter
{
    fn compute(
        &mut self,
        (text, color, syntax): (&'a str, Color32, crate::theme::SyntaxPalette),
    ) -> egui::text::LayoutJob {
        highlight_with_palette(text, color, &syntax)
    }
}
/// Reuse tokenisation when repainting unchanged documents.
pub fn cached_highlight(ctx: &egui::Context, text: &str, color: Color32) -> egui::text::LayoutJob {
    let syntax = crate::theme::current(ctx).syntax;
    ctx.memory_mut(|m| {
        m.caches
            .cache::<egui::util::cache::FrameCache<egui::text::LayoutJob, Highlighter>>()
            .get((text, color, syntax))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cached_syntax_updates_immediately_when_the_theme_changes() {
        let ctx = egui::Context::default();
        let settings = crate::theme::Settings::default();
        crate::theme::apply(&ctx, &settings);
        let old = cached_highlight(&ctx, "class", Color32::WHITE);
        let settings = crate::theme::Settings {
            preset: crate::theme::Preset::Dawn,
            custom: None,
        };
        crate::theme::apply(&ctx, &settings);
        let new = cached_highlight(&ctx, "class", Color32::BLACK);
        assert_ne!(old.sections[0].format.color, new.sections[0].format.color);
        assert_eq!(
            new.sections[0].format.color,
            settings.definition().syntax.keyword.color()
        );
    }
    #[test]
    fn completions_stay_out_of_comments_and_strings() {
        assert!(!completion_context("// str", 6));
        assert!(!completion_context("/* str", 6));
        assert!(!completion_context("\"str", 4));
        assert!(completion_context("π str", 5));
    }
    #[test]
    fn highlights_keywords_and_names() {
        let tokens = tokenize("public class Hello { }");
        assert!(tokens.contains(&(TokenKind::Modifier, "public")));
        assert!(tokens.contains(&(TokenKind::Keyword, "class")));
        assert!(tokens.contains(&(TokenKind::Plain, "Hello")));
    }
    #[test]
    fn comments_do_not_highlight_embedded_keywords() {
        assert_eq!(
            tokenize("// public class"),
            vec![(TokenKind::Comment, "// public class")]
        );
    }
    #[test]
    fn strings_and_escapes() {
        let tokens = tokenize("\"hello \\\"class\\\"\" // trailing");
        assert_eq!(tokens[0].0, TokenKind::String);
        assert_eq!(tokens.last().unwrap().0, TokenKind::Comment);
    }
    #[test]
    fn preserves_unicode_and_entire_input() {
        let sample = "π = \"✓\"; // 世界\n";
        assert_eq!(
            tokenize(sample).iter().map(|(_, s)| *s).collect::<String>(),
            sample
        );
    }
    #[test]
    fn completion_deduplicates_and_filters() {
        let words = completion_candidates("class Camera {} Camera camera", "Cam");
        assert_eq!(words, vec!["Camera"]);
        assert!(completion_candidates("class Camera", "zzzz").is_empty());
    }
    #[test]
    fn tokenizes_unterminated_comment_and_string() {
        assert_eq!(tokenize("/* open")[0].0, TokenKind::Comment);
        assert_eq!(tokenize("\"open")[0].0, TokenKind::String);
    }
    #[test]
    fn numeric_and_modifiers() {
        let t = tokenize("static int x = 0xDEAD; bool y = true;");
        assert!(t.contains(&(TokenKind::Modifier, "static")));
        assert!(t.contains(&(TokenKind::Number, "0xDEAD")));
        assert!(t.contains(&(TokenKind::Literal, "true")));
    }
    #[test]
    fn completion_replaces_at_caret_unicode_safe() {
        let mut text = "π st world".to_string();
        assert_eq!(prefix_at(&text, 4), "st");
        let index = insert_completion(&mut text, 4, "st", "static");
        assert_eq!(index, 8);
        assert_eq!(text, "π static world");
    }
    #[test]
    fn completion_examples_include_all_three() {
        let results = completion_candidates("", "st");
        assert!(results.contains(&"static".to_string()));
        assert!(results.contains(&"string".to_string()));
        assert!(results.contains(&"struct".to_string()));
    }

    #[test]
    fn syntax_cache_keeps_text_and_foreground_entries_independent() {
        let ctx = egui::Context::default();
        let one = cached_highlight(&ctx, "plain_name", Color32::from_rgb(1, 2, 3));
        let two = cached_highlight(&ctx, "plain_name", Color32::from_rgb(4, 5, 6));
        assert_eq!(one.text, two.text);
        assert_ne!(one.sections[0].format.color, two.sections[0].format.color);
        let other = cached_highlight(&ctx, "another_name", Color32::from_rgb(1, 2, 3));
        assert_eq!(other.text, "another_name");
        assert_eq!(one.text, "plain_name");
    }
}
