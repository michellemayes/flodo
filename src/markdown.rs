//! Markdown → egui.
//!
//! Hand-rolled on `pulldown-cmark` rather than delegated to a widget crate,
//! because every size and colour here has to derive from the user's chosen
//! font, font size, and accent — which is the whole point of the app.

use crate::theme::Palette;
use eframe::egui::{self, text::LayoutJob, Color32, FontFamily, FontId, TextFormat};
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

pub const FAMILY_UI: &str = "ui";
pub const FAMILY_BOLD: &str = "ui-bold";
pub const FAMILY_ITALIC: &str = "ui-italic";
pub const FAMILY_BOLD_ITALIC: &str = "ui-bold-italic";
pub const FAMILY_MONO: &str = "mono";

fn family(name: &str) -> FontFamily {
    FontFamily::Name(name.into())
}

/// Styling of one contiguous run of text.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Style {
    pub strong: bool,
    pub emph: bool,
    pub code: bool,
    pub strike: bool,
    pub link: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub text: String,
    pub style: Style,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Paragraph(Vec<Span>),
    Heading {
        level: u8,
        spans: Vec<Span>,
    },
    Item {
        depth: u8,
        marker: Option<u64>,
        spans: Vec<Span>,
    },
    Code {
        lang: String,
        text: String,
    },
    Quote(Vec<Span>),
    Rule,
}

/// Tracks one open list item so a nested list can't clobber its parent's text.
struct ItemFrame {
    depth: u8,
    marker: Option<u64>,
    flushed: bool,
}

fn options() -> Options {
    // Strikethrough only. Tasklists stay off — nested todos are exactly the
    // project-management creep this app refuses.
    Options::ENABLE_STRIKETHROUGH
}

/// Parses markdown into a flat block list.
pub fn parse(src: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut spans: Vec<Span> = Vec::new();
    let mut style = Style::default();
    let mut list_stack: Vec<Option<u64>> = Vec::new();
    let mut item_stack: Vec<ItemFrame> = Vec::new();
    let mut heading: Option<u8> = None;
    let mut in_quote = false;
    let mut code: Option<(String, String)> = None;

    let push = |spans: &mut Vec<Span>, text: &str, style: &Style| {
        if text.is_empty() {
            return;
        }
        // Merge with the previous run when the styling is identical, so a
        // paragraph doesn't become dozens of one-character sections.
        if let Some(last) = spans.last_mut() {
            if last.style == *style {
                last.text.push_str(text);
                return;
            }
        }
        spans.push(Span {
            text: text.to_string(),
            style: style.clone(),
        });
    };

    for event in Parser::new_ext(src, options()) {
        match event {
            Event::Start(Tag::Paragraph) => spans.clear(),
            Event::End(TagEnd::Paragraph) => {
                if !spans.is_empty() {
                    // A paragraph inside a list item *is* the item's content
                    // (loose lists wrap item text in a paragraph).
                    let block = match item_stack.last_mut() {
                        Some(frame) => {
                            frame.flushed = true;
                            Block::Item {
                                depth: frame.depth,
                                marker: frame.marker,
                                spans: std::mem::take(&mut spans),
                            }
                        }
                        None if in_quote => Block::Quote(std::mem::take(&mut spans)),
                        None => Block::Paragraph(std::mem::take(&mut spans)),
                    };
                    blocks.push(block);
                }
            }

            Event::Start(Tag::Heading { level, .. }) => {
                spans.clear();
                heading = Some(match level {
                    HeadingLevel::H1 => 1,
                    HeadingLevel::H2 => 2,
                    _ => 3,
                });
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some(level) = heading.take() {
                    blocks.push(Block::Heading {
                        level,
                        spans: std::mem::take(&mut spans),
                    });
                }
            }

            Event::Start(Tag::BlockQuote(_)) => in_quote = true,
            Event::End(TagEnd::BlockQuote(_)) => in_quote = false,

            Event::Start(Tag::List(start)) => {
                // A nested list interrupts its parent item, so emit whatever
                // the parent has accumulated before descending — otherwise the
                // parent's own text is lost.
                if let Some(frame) = item_stack.last_mut() {
                    if !spans.is_empty() {
                        frame.flushed = true;
                        blocks.push(Block::Item {
                            depth: frame.depth,
                            marker: frame.marker,
                            spans: std::mem::take(&mut spans),
                        });
                    }
                }
                list_stack.push(start);
            }
            Event::End(TagEnd::List(_)) => {
                list_stack.pop();
            }
            Event::Start(Tag::Item) => {
                spans.clear();
                item_stack.push(ItemFrame {
                    depth: list_stack.len().saturating_sub(1) as u8,
                    marker: list_stack.last().copied().flatten(),
                    flushed: false,
                });
            }
            Event::End(TagEnd::Item) => {
                if let Some(frame) = item_stack.pop() {
                    if !frame.flushed || !spans.is_empty() {
                        blocks.push(Block::Item {
                            depth: frame.depth,
                            marker: frame.marker,
                            spans: std::mem::take(&mut spans),
                        });
                    }
                }
                if let Some(Some(n)) = list_stack.last_mut() {
                    *n += 1;
                }
            }

            Event::Start(Tag::CodeBlock(kind)) => {
                let lang = match kind {
                    CodeBlockKind::Fenced(l) => l.to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
                code = Some((lang, String::new()));
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some((lang, text)) = code.take() {
                    blocks.push(Block::Code {
                        lang,
                        text: text.trim_end_matches('\n').to_string(),
                    });
                }
            }

            Event::Start(Tag::Strong) => style.strong = true,
            Event::End(TagEnd::Strong) => style.strong = false,
            Event::Start(Tag::Emphasis) => style.emph = true,
            Event::End(TagEnd::Emphasis) => style.emph = false,
            Event::Start(Tag::Strikethrough) => style.strike = true,
            Event::End(TagEnd::Strikethrough) => style.strike = false,
            Event::Start(Tag::Link { dest_url, .. }) => style.link = Some(dest_url.to_string()),
            Event::End(TagEnd::Link) => style.link = None,

            Event::Text(t) => match code.as_mut() {
                Some((_, buf)) => buf.push_str(&t),
                None => push(&mut spans, &t, &style),
            },
            Event::Code(t) => {
                let mut s = style.clone();
                s.code = true;
                push(&mut spans, &t, &s);
            }
            Event::SoftBreak => push(&mut spans, " ", &style),
            Event::HardBreak => push(&mut spans, "\n", &style),
            Event::Rule => blocks.push(Block::Rule),

            // Raw HTML and anything else unsupported renders as plain text
            // rather than silently disappearing.
            Event::Html(t) | Event::InlineHtml(t) => push(&mut spans, &t, &style),
            _ => {}
        }
    }

    // An unclosed construct (e.g. a fence with no terminator) leaves content
    // buffered; flush it instead of dropping it.
    if let Some((lang, text)) = code.take() {
        blocks.push(Block::Code {
            lang,
            text: text.trim_end_matches('\n').to_string(),
        });
    }
    if !spans.is_empty() {
        blocks.push(Block::Paragraph(spans));
    }

    blocks
}

/// Everything the renderer needs that isn't in the markdown itself.
#[derive(Clone, Copy)]
pub struct Ctx<'a> {
    pub palette: &'a Palette,
    pub size: f32,
    pub dim: bool,
}

impl Ctx<'_> {
    fn text_color(&self) -> Color32 {
        if self.dim {
            self.palette.muted
        } else {
            self.palette.text
        }
    }

    fn font_for(&self, style: &Style, size: f32) -> FontId {
        let name = if style.code {
            FAMILY_MONO
        } else {
            match (style.strong, style.emph) {
                (true, true) => FAMILY_BOLD_ITALIC,
                (true, false) => FAMILY_BOLD,
                (false, true) => FAMILY_ITALIC,
                (false, false) => FAMILY_UI,
            }
        };
        FontId::new(size, family(name))
    }

    fn format(&self, style: &Style, size: f32) -> TextFormat {
        let color = if style.link.is_some() {
            self.palette.accent
        } else {
            self.text_color()
        };
        TextFormat {
            font_id: self.font_for(style, if style.code { size - 1.0 } else { size }),
            color,
            background: if style.code {
                self.palette.surface
            } else {
                Color32::TRANSPARENT
            },
            strikethrough: if style.strike {
                egui::Stroke::new(1.0, color)
            } else {
                egui::Stroke::NONE
            },
            underline: if style.link.is_some() {
                egui::Stroke::new(1.0, color)
            } else {
                egui::Stroke::NONE
            },
            line_height: Some(size * 1.45),
            ..Default::default()
        }
    }
}

/// Lays a run of spans into a single job so wrapping is correct across mixed
/// styles (many small labels would each wrap independently).
pub fn job(spans: &[Span], ctx: &Ctx, size: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    for span in spans {
        job.append(&span.text, 0.0, ctx.format(&span.style, size));
    }
    job
}

/// Renders a single line of markdown (a todo title).
pub fn inline_job(src: &str, ctx: &Ctx) -> LayoutJob {
    let spans = parse(src)
        .into_iter()
        .flat_map(|b| match b {
            Block::Paragraph(s) | Block::Heading { spans: s, .. } | Block::Quote(s) => s,
            Block::Item { spans, .. } => spans,
            Block::Code { text, .. } => vec![Span {
                text,
                style: Style {
                    code: true,
                    ..Default::default()
                },
            }],
            Block::Rule => vec![],
        })
        .collect::<Vec<_>>();

    if spans.is_empty() {
        // A title of only markdown punctuation should still show something.
        let mut job = LayoutJob::default();
        job.append(src, 0.0, ctx.format(&Style::default(), ctx.size));
        return job;
    }
    job(&spans, ctx, ctx.size)
}

/// Renders a todo body.
///
/// `max_w` is supplied by the caller because a `Ui` nested in horizontal
/// layouts reports a width far larger than the column it is actually drawn
/// into, which would let long lines run past the window edge.
pub fn render(ui: &mut egui::Ui, blocks: &[Block], ctx: &Ctx, salt: u64, max_w: f32) {
    let p = ctx.palette;
    let body = ctx.size - 1.0;
    let full_w = max_w;
    ui.set_max_width(max_w);

    for (ix, block) in blocks.iter().enumerate() {
        match block {
            Block::Paragraph(spans) => {
                paragraph(ui, spans, ctx, body, full_w);
            }

            Block::Heading { level, spans } => {
                ui.add_space(6.0);
                let scale = match level {
                    1 => 1.35,
                    2 => 1.15,
                    _ => 1.0,
                };
                let mut bold: Vec<Span> = spans.clone();
                for s in &mut bold {
                    s.style.strong = true;
                }
                let mut j = job(&bold, ctx, body * scale);
                j.wrap.max_width = ui.available_width();
                ui.label(j);
            }

            Block::Item {
                depth,
                marker,
                spans,
            } => {
                let indent = 16.0 * (*depth as f32 + 1.0);
                ui.horizontal_top(|ui| {
                    ui.add_space(indent);
                    match marker {
                        Some(n) => {
                            ui.label(
                                egui::RichText::new(format!("{n}."))
                                    .color(p.muted)
                                    .font(FontId::new(body, family(FAMILY_MONO))),
                            );
                        }
                        None => {
                            let (r, _) = ui.allocate_exact_size(
                                egui::vec2(10.0, body * 1.45),
                                egui::Sense::hover(),
                            );
                            ui.painter().circle_filled(r.center(), 2.5, p.muted);
                        }
                    }
                    ui.add_space(2.0);
                    paragraph(ui, spans, ctx, body, (full_w - indent - 14.0).max(40.0));
                });
            }

            Block::Quote(spans) => {
                ui.horizontal_top(|ui| {
                    let (bar, _) =
                        ui.allocate_exact_size(egui::vec2(2.0, body * 1.6), egui::Sense::hover());
                    ui.painter()
                        .rect_filled(bar, 1.0, p.accent.gamma_multiply(0.4));
                    ui.add_space(8.0);
                    let mut dimmed = *ctx;
                    dimmed.dim = true;
                    paragraph(ui, spans, &dimmed, body, (full_w - 10.0).max(40.0));
                });
            }

            Block::Code { lang, text } => {
                ui.add_space(4.0);
                code_block(ui, lang, text, ctx, salt.wrapping_add(ix as u64), full_w);
                ui.add_space(4.0);
            }

            Block::Rule => {
                ui.add_space(6.0);
                let (r, _) = ui.allocate_exact_size(egui::vec2(full_w, 1.0), egui::Sense::hover());
                ui.painter().rect_filled(r, 0.0, p.border);
                ui.add_space(6.0);
            }
        }
    }
}

/// Links need real hit-testing, so a paragraph containing them is laid out as
/// wrapped alternating pieces instead of one job.
/// `max_w` is passed in rather than read from the `Ui`: inside a horizontal
/// layout `available_width()` does not reflect the column the caller intends,
/// and a `LayoutJob` never inherits the `Ui`'s wrap width anyway.
fn paragraph(ui: &mut egui::Ui, spans: &[Span], ctx: &Ctx, size: f32, max_w: f32) {
    if spans.iter().all(|s| s.style.link.is_none()) {
        let mut j = job(spans, ctx, size);
        j.wrap.max_width = max_w;
        // Inside a horizontal layout egui defaults Labels to `Extend`, which
        // silently overrides the job's wrap width. Say what we mean.
        ui.add(egui::Label::new(j).wrap_mode(egui::TextWrapMode::Wrap));
        return;
    }
    ui.set_max_width(max_w);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for span in spans {
            match &span.style.link {
                Some(url) => {
                    let single = [span.clone()];
                    let j = job(&single, ctx, size);
                    if ui.link(j).clicked() {
                        ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                    }
                }
                None => {
                    let single = [span.clone()];
                    ui.label(job(&single, ctx, size));
                }
            }
        }
    });
}

fn code_block(ui: &mut egui::Ui, lang: &str, text: &str, ctx: &Ctx, salt: u64, max_w: f32) {
    let p = ctx.palette;
    let size = ctx.size - 1.0;

    egui::Frame::NONE
        .fill(p.surface)
        .stroke(egui::Stroke::new(1.0, p.border))
        .corner_radius(6)
        .inner_margin(8.0)
        .show(ui, |ui| {
            ui.set_width((max_w - 18.0).max(40.0));

            if !lang.is_empty() {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(lang)
                            .color(p.muted)
                            .size(size * 0.8)
                            .family(family(FAMILY_MONO)),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .small_button("copy")
                            .on_hover_text("Copy this snippet")
                            .clicked()
                        {
                            ui.ctx().copy_text(text.to_string());
                        }
                    });
                });
            }

            // Code scrolls horizontally; wrapped code is unreadable.
            egui::ScrollArea::horizontal()
                .id_salt(("code", salt))
                .max_height(f32::INFINITY)
                .show(ui, |ui| {
                    let mut j = LayoutJob::default();
                    j.wrap.max_width = f32::INFINITY;
                    j.append(
                        text,
                        0.0,
                        TextFormat {
                            font_id: FontId::new(size, family(FAMILY_MONO)),
                            color: ctx.text_color(),
                            line_height: Some(size * 1.4),
                            ..Default::default()
                        },
                    );
                    // Extend, not wrap: the Label would otherwise re-impose the
                    // available width and wrap the code anyway.
                    ui.add(egui::Label::new(j).wrap_mode(egui::TextWrapMode::Extend));
                });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_of(spans: &[Span]) -> String {
        spans.iter().map(|s| s.text.as_str()).collect()
    }

    #[test]
    fn plain_text_is_one_paragraph() {
        let b = parse("hello world");
        assert_eq!(b.len(), 1);
        match &b[0] {
            Block::Paragraph(s) => assert_eq!(text_of(s), "hello world"),
            other => panic!("expected paragraph, got {other:?}"),
        }
    }

    #[test]
    fn bold_italic_and_strikethrough_are_captured() {
        let b = parse("a **bold** _em_ ~~gone~~");
        let Block::Paragraph(spans) = &b[0] else {
            panic!("expected paragraph")
        };
        let bold = spans.iter().find(|s| s.style.strong).unwrap();
        assert_eq!(bold.text, "bold");
        let em = spans.iter().find(|s| s.style.emph).unwrap();
        assert_eq!(em.text, "em");
        let st = spans.iter().find(|s| s.style.strike).unwrap();
        assert_eq!(st.text, "gone");
    }

    #[test]
    fn inline_code_is_marked_but_not_a_code_block() {
        let b = parse("run `cargo test` now");
        let Block::Paragraph(spans) = &b[0] else {
            panic!()
        };
        let code = spans.iter().find(|s| s.style.code).unwrap();
        assert_eq!(code.text, "cargo test");
        assert!(!b.iter().any(|x| matches!(x, Block::Code { .. })));
    }

    #[test]
    fn fenced_code_keeps_its_language_and_exact_text() {
        let b = parse("```rust\nfn main() {\n    println!(\"hi\");\n}\n```");
        let Block::Code { lang, text } = &b[0] else {
            panic!("expected code block, got {:?}", b)
        };
        assert_eq!(lang, "rust");
        assert_eq!(text, "fn main() {\n    println!(\"hi\");\n}");
    }

    #[test]
    fn a_fence_with_no_language_still_parses() {
        let b = parse("```\nplain\n```");
        assert!(matches!(&b[0], Block::Code { lang, text } if lang.is_empty() && text == "plain"));
    }

    #[test]
    fn an_unclosed_fence_still_yields_its_content() {
        let b = parse("```rust\nfn main() {}");
        assert!(
            matches!(&b[0], Block::Code { text, .. } if text.contains("fn main")),
            "got {b:?}"
        );
    }

    #[test]
    fn lists_track_depth_and_ordinals() {
        let b = parse("- one\n- two\n  - nested");
        let items: Vec<_> = b
            .iter()
            .filter_map(|x| match x {
                Block::Item { depth, spans, .. } => Some((*depth, text_of(spans))),
                _ => None,
            })
            .collect();
        assert_eq!(items[0], (0, "one".into()));
        // Regression: the nested list used to clobber its parent's text.
        assert_eq!(items[1], (0, "two".into()));
        assert_eq!(items[2], (1, "nested".into()));
    }

    #[test]
    fn loose_lists_keep_their_item_text() {
        // Blank lines between items make pulldown wrap each item in a
        // paragraph, which must still surface as an Item, not a Paragraph.
        let b = parse("- one\n\n- two\n");
        let items: Vec<_> = b
            .iter()
            .filter_map(|x| match x {
                Block::Item { spans, .. } => Some(text_of(spans)),
                _ => None,
            })
            .collect();
        assert_eq!(items, vec!["one".to_string(), "two".to_string()]);
        assert!(
            !b.iter().any(|x| matches!(x, Block::Paragraph(_))),
            "item text must not leak out as a bare paragraph: {b:?}"
        );
    }

    #[test]
    fn ordered_lists_number_upward() {
        let b = parse("1. first\n2. second\n3. third");
        let markers: Vec<_> = b
            .iter()
            .filter_map(|x| match x {
                Block::Item { marker, .. } => *marker,
                _ => None,
            })
            .collect();
        assert_eq!(markers, vec![1, 2, 3]);
    }

    #[test]
    fn headings_carry_their_level() {
        let b = parse("# one\n## two\n### three");
        let levels: Vec<_> = b
            .iter()
            .filter_map(|x| match x {
                Block::Heading { level, .. } => Some(*level),
                _ => None,
            })
            .collect();
        assert_eq!(levels, vec![1, 2, 3]);
    }

    #[test]
    fn links_and_quotes_and_rules_are_recognised() {
        let b = parse("[docs](https://example.com)\n\n> quoted\n\n---");
        let Block::Paragraph(spans) = &b[0] else {
            panic!()
        };
        assert_eq!(spans[0].style.link.as_deref(), Some("https://example.com"));
        assert!(b.iter().any(|x| matches!(x, Block::Quote(_))));
        assert!(b.iter().any(|x| matches!(x, Block::Rule)));
    }

    #[test]
    fn adjacent_runs_with_identical_styling_are_merged() {
        let Block::Paragraph(spans) = &parse("one two three")[0] else {
            panic!()
        };
        assert_eq!(spans.len(), 1, "should not fragment: {spans:?}");
    }

    #[test]
    fn raw_html_shows_as_text_rather_than_vanishing() {
        let b = parse("<div>hi</div>");
        let all: String = b
            .iter()
            .filter_map(|x| match x {
                Block::Paragraph(s) | Block::Quote(s) => Some(text_of(s)),
                _ => None,
            })
            .collect();
        assert!(all.contains("div"), "got {b:?}");
    }

    /// The body is free-form user text, so the parser must never panic.
    #[test]
    fn pathological_input_does_not_panic() {
        let deep_list = (0..100)
            .map(|i| format!("{}- x", " ".repeat(i * 2)))
            .collect::<Vec<_>>()
            .join("\n");
        let cases = vec![
            String::new(),
            "*".into(),
            "**".into(),
            "***".into(),
            "`".into(),
            "```".into(),
            "```rust".into(),
            "> ".into(),
            "[".into(),
            "[]()".into(),
            "![](".into(),
            "#".repeat(50),
            "~".repeat(50),
            "a".repeat(100_000),
            "\0\u{feff}\u{202e}".into(),
            "🎉 emoji ✅ test".into(),
            deep_list,
            "|a|b|\n|-|-|\n|1|2|".into(),
        ];
        for c in cases {
            let _ = parse(&c);
        }
    }

    #[test]
    fn empty_input_produces_no_blocks() {
        assert!(parse("").is_empty());
        assert!(parse("   \n  \n").is_empty());
    }
}
