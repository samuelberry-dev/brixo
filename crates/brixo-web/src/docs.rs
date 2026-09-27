//! The Learn section of the website: Brixo's guide to making games, written
//! in Markdown (`crates/brixo-web/docs/*.md`) and turned into pages here.
//!
//! Code in ```rovik blocks is real: `tests/docs.rs` checks every block
//! parses, and runs the ones marked `run` in a real game, so the guide can't
//! teach something that doesn't work. A block's info string can say what the
//! script sits in and what else the example needs:
//!
//! ```text
//! ```rovik run in=part:Lava with=textlabel:Score,part:Door
//! ```
//!
//! (`in` is the object the script goes inside, so `self`; `with` are other
//! objects it finds by name. Shown to readers as a plain Rovik block.)

use std::collections::HashMap;
use std::sync::OnceLock;

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

/// One page of the guide.
pub struct Page {
    pub slug: &'static str,
    pub section: &'static str,
    pub title: &'static str,
    pub markdown: &'static str,
}

macro_rules! pages {
    ($($section:literal => [$(($slug:literal, $title:literal)),* $(,)?]),* $(,)?) => {
        pub const PAGES: &[Page] = &[
            $($(Page { slug: $slug, section: $section, title: $title, markdown: include_str!(concat!("../docs/", $slug, ".md")) },)*)*
        ];
    };
}

pages! {
    "Getting started" => [
        ("welcome", "Welcome"),
        ("install", "Get Brixo Studio"),
        ("studio-tour", "A tour of Studio"),
        ("first-game", "Your first game"),
        ("publishing", "Publish and play with friends"),
    ],
    "Scripting with Rovik" => [
        ("rovik-basics", "Rovik basics"),
        ("rovik-logic", "Decisions and loops"),
        ("rovik-lists-maps", "Lists and maps"),
        ("rovik-functions", "Functions"),
        ("events", "Events, waiting and time"),
        ("errors", "Reading errors"),
    ],
    "Building worlds" => [
        ("parts", "Parts"),
        ("models-folders", "Models, folders and templates"),
        ("spawns-teams", "Spawns and teams"),
        ("physics", "Physics, explosions and breaking"),
    ],
    "Players and interface" => [
        ("players", "Players"),
        ("gui", "On-screen interface (GUI)"),
        ("tools", "Tools and weapons"),
        ("sounds", "Sounds and music"),
    ],
    "How-tos" => [
        ("howto-kill-brick", "Kill bricks and lava"),
        ("howto-coins", "Coins and a leaderboard"),
        ("howto-door", "A door with a button"),
        ("howto-moving-platform", "Moving platforms"),
        ("howto-teleporter", "Teleporters"),
        ("howto-checkpoints", "An obby with checkpoints"),
        ("howto-pads", "Jump pads and speed pads"),
        ("howto-shop", "A shop"),
        ("howto-rounds", "Timed rounds"),
        ("howto-teams", "A team game"),
        ("howto-gun", "Make your own gun"),
    ],
    "Reference" => [
        ("ref-language", "Language"),
        ("ref-functions", "Functions"),
        ("ref-events", "Events"),
        ("ref-objects", "Objects and properties"),
        ("ref-names", "Names: sounds, materials and more"),
        ("samples", "The sample games, explained"),
    ],
}

macro_rules! images {
    ($($name:literal),* $(,)?) => {
        /// Pictures the pages show (docs/img).
        pub const IMAGES: &[(&str, &[u8])] = &[$(($name, include_bytes!(concat!("../docs/img/", $name))),)*];
    };
}

images! {
    "studio-overview.png",
    "studio-explorer.png",
    "studio-properties.png",
    "studio-script.png",
    "studio-output.png",
    "first-game-parts.png",
    "first-game-play.png",
    "parts-shapes.png",
    "parts-materials.png",
    "gui-example.png",
    "tools-kit.png",
    "howto-coins.png",
    "howto-door.png",
    "howto-checkpoints.png",
    "howto-rounds.png",
    "howto-gun.png",
    "samples.png",
}

/// A code example in a page: its source, and what the test harness needs.
pub struct Example {
    pub page: &'static str,
    pub code: String,
    /// Run it in a real game (not just check it parses).
    pub run: bool,
    /// A deliberate mistake, shown to explain an error: it mustn't parse.
    pub broken: bool,
    /// A fragment with `...` in it, for the shape of something: not checked.
    pub sketch: bool,
    /// What the script sits inside: (class, name).
    pub inside: Option<(String, String)>,
    /// Other things it expects to find: (class, name).
    pub with: Vec<(String, String)>,
}

/// Every ```rovik block in the guide.
pub fn examples() -> Vec<Example> {
    let mut out = Vec::new();
    for page in PAGES {
        let mut current: Option<Example> = None;
        for event in Parser::new_ext(page.markdown, options()) {
            match event {
                Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) => {
                    let mut words = info.split_whitespace();
                    if words.next() != Some("rovik") {
                        continue;
                    }
                    let mut ex = Example { page: page.slug, code: String::new(), run: false, broken: false, sketch: false, inside: None, with: Vec::new() };
                    for w in words {
                        let pair = |s: &str| {
                            let (c, n) = s.split_once(':').unwrap_or(("part", s));
                            (c.to_string(), n.replace('_', " "))
                        };
                        if w == "run" {
                            ex.run = true;
                        } else if w == "broken" {
                            ex.broken = true;
                        } else if w == "sketch" {
                            ex.sketch = true;
                        } else if let Some(v) = w.strip_prefix("in=") {
                            ex.inside = Some(pair(v));
                        } else if let Some(v) = w.strip_prefix("with=") {
                            ex.with.extend(v.split(',').map(pair));
                        }
                    }
                    current = Some(ex);
                }
                Event::Text(t) => {
                    if let Some(ex) = current.as_mut() {
                        ex.code.push_str(&t);
                    }
                }
                Event::End(TagEnd::CodeBlock) => {
                    if let Some(ex) = current.take() {
                        out.push(ex);
                    }
                }
                _ => {}
            }
        }
    }
    out
}

fn options() -> Options {
    Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// "Moving parts with scripts" -> "moving-parts-with-scripts".
fn anchor(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_string()
}

/// A page's body as HTML, and its section headings (for the page's
/// contents list and the search).
fn body_html(markdown: &str) -> (String, Vec<(String, String)>) {
    let mut html = String::new();
    let mut headings = Vec::new();
    let mut events: Vec<Event> = Vec::new();
    let mut in_code: Option<String> = None;
    let mut code = String::new();
    let mut heading: Option<(HeadingLevel, String)> = None;
    for event in Parser::new_ext(markdown, options()) {
        match event {
            Event::Start(Tag::CodeBlock(kind)) => {
                let lang = match kind {
                    CodeBlockKind::Fenced(info) => info.split_whitespace().next().unwrap_or("").to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
                in_code = Some(lang);
                code.clear();
            }
            Event::End(TagEnd::CodeBlock) => {
                let lang = in_code.take().unwrap_or_default();
                let class = if lang == "rovik" { "code rovik" } else { "code" };
                events.push(Event::Html(
                    format!("<div class=\"codebox\"><pre class=\"{class}\"><code>{}</code></pre></div>\n", escape(code.trim_end_matches('\n')))
                        .into(),
                ));
            }
            Event::Text(t) if in_code.is_some() => code.push_str(&t),
            Event::Start(Tag::Heading { level, .. }) => {
                heading = Some((level, String::new()));
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some((level, text)) = heading.take() {
                    let id = anchor(&text);
                    let n = match level {
                        HeadingLevel::H1 => 1,
                        HeadingLevel::H2 => 2,
                        _ => 3,
                    };
                    if n == 2 {
                        headings.push((id.clone(), text.clone()));
                    }
                    events.push(Event::Html(format!("<h{n} id=\"{id}\"><a class=\"hash\" href=\"#{id}\">#</a>{}</h{n}>\n", escape(&text)).into()));
                }
            }
            Event::Text(t) | Event::Code(t) if heading.is_some() => heading.as_mut().unwrap().1.push_str(&t),
            other if heading.is_some() => drop(other),
            // Pictures: shown as a framed figure, the alt text as its caption.
            Event::Start(Tag::Image { dest_url, .. }) => {
                events.push(Event::Html(format!("<figure><img src=\"{}\" alt=\"", escape(&dest_url)).into()));
                events.push(Event::Html("CAPTION_START".into()));
            }
            Event::End(TagEnd::Image) => events.push(Event::Html("CAPTION_END".into())),
            other => events.push(other),
        }
    }
    // Image alt text arrives as Text events between the markers: use it twice,
    // as the alt and as the caption.
    let mut fixed: Vec<Event> = Vec::new();
    let mut caption: Option<String> = None;
    for e in events {
        match (&e, caption.as_mut()) {
            (Event::Html(h), _) if h.as_ref() == "CAPTION_START" => caption = Some(String::new()),
            (Event::Html(h), Some(c)) if h.as_ref() == "CAPTION_END" => {
                let c = escape(c);
                fixed.push(Event::Html(format!("{c}\" loading=\"lazy\"><figcaption>{c}</figcaption></figure>").into()));
                caption = None;
            }
            (Event::Text(t), Some(c)) => c.push_str(t),
            (Event::Code(t), Some(c)) => c.push_str(t),
            (_, Some(_)) => {}
            _ => fixed.push(e),
        }
    }
    pulldown_cmark::html::push_html(&mut html, fixed.into_iter());
    (html, headings)
}

/// Every page, rendered once.
struct Built {
    pages: HashMap<&'static str, String>,
    search: String,
}

fn built() -> &'static Built {
    static BUILT: OnceLock<Built> = OnceLock::new();
    BUILT.get_or_init(|| {
        let bodies: Vec<(String, Vec<(String, String)>)> = PAGES.iter().map(|p| body_html(p.markdown)).collect();
        let mut pages = HashMap::new();
        for (i, page) in PAGES.iter().enumerate() {
            pages.insert(page.slug, page_html(i, &bodies[i].0, &bodies[i].1));
        }
        // For the sidebar search: each page's title, section, headings and words.
        let search = PAGES
            .iter()
            .zip(&bodies)
            .map(|(p, (html, heads))| {
                let text = strip_tags(html);
                format!(
                    "{{\"slug\":{},\"title\":{},\"heads\":{},\"text\":{}}}",
                    json(p.slug),
                    json(p.title),
                    json(&heads.iter().map(|(_, h)| h.as_str()).collect::<Vec<_>>().join(" · ")),
                    json(&text)
                )
            })
            .collect::<Vec<_>>()
            .join(",\n");
        Built { pages, search: format!("[{search}]") }
    })
}

fn json(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push(' '),
            c if (c as u32) < 0x20 => {}
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn strip_tags(html: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for c in html.chars() {
        match c {
            '<' => inside = true,
            '>' => {
                inside = false;
                out.push(' ');
            }
            c if !inside => out.push(c),
            _ => {}
        }
    }
    out.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&amp;", "&").split_whitespace().collect::<Vec<_>>().join(" ")
}

fn page_html(index: usize, body: &str, headings: &[(String, String)]) -> String {
    let page = &PAGES[index];
    let mut side = String::new();
    let mut section = "";
    for p in PAGES {
        if p.section != section {
            if !section.is_empty() {
                side.push_str("</ul>");
            }
            section = p.section;
            side.push_str(&format!("<div class=\"sec\">{}</div><ul>", escape(section)));
        }
        let on = if p.slug == page.slug { " class=\"on\"" } else { "" };
        side.push_str(&format!("<li><a{on} href=\"/learn/{}\">{}</a></li>", p.slug, escape(p.title)));
    }
    side.push_str("</ul>");
    let toc = if headings.len() >= 3 {
        let items: String = headings.iter().map(|(id, t)| format!("<li><a href=\"#{id}\">{}</a></li>", escape(t))).collect();
        format!("<div class=\"toc\"><b>On this page</b><ul>{items}</ul></div>")
    } else {
        String::new()
    };
    let prev = index.checked_sub(1).map(|i| &PAGES[i]);
    let next = PAGES.get(index + 1);
    let pager = format!(
        "<div class=\"pager\">{}{}</div>",
        prev.map(|p| format!("<a class=\"prev\" href=\"/learn/{}\">&larr; {}</a>", p.slug, escape(p.title))).unwrap_or_default(),
        next.map(|p| format!("<a class=\"next\" href=\"/learn/{}\">{} &rarr;</a>", p.slug, escape(p.title))).unwrap_or_default()
    );
    format!(
        r#"<!DOCTYPE html><html lang="en"><head><meta charset="utf-8"><title>{title} &middot; Learn Brixo</title>
<meta name="viewport" content="width=device-width, initial-scale=1">
<link rel="icon" href="/favicon.svg"><link rel="stylesheet" href="/app.css"><script src="/app.js"></script></head>
<body><div id="page"><div id="banner"></div><div id="nav"></div>
<div id="content"><div class="learn">
<nav class="learn-side"><div class="box navy"><h2>Learn Brixo</h2><div class="inner">
<input id="learn-search" type="search" placeholder="Search the guide" aria-label="Search the guide">
<div id="learn-results"></div><div id="learn-index">{side}</div></div></div></nav>
<article class="learn-main box"><h2>{section}</h2><div class="inner doc">
<h1>{title}</h1>{toc}{body}{pager}</div></article>
</div></div>
<div id="footer"></div></div>
<script>shell("learn"); learnPage();</script></body></html>"#,
        title = escape(page.title),
        section = escape(page.section),
    )
}

/// The page at /learn/<slug>, if there is one.
pub fn page(slug: &str) -> Option<&'static str> {
    built().pages.get(slug).map(String::as_str)
}

/// The search index, for the sidebar's search box.
pub fn search_index() -> &'static str {
    &built().search
}

/// A picture by file name.
pub fn image(name: &str) -> Option<&'static [u8]> {
    IMAGES.iter().find(|(n, _)| *n == name).map(|(_, b)| *b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headings_get_anchors_and_code_is_escaped() {
        let (html, heads) = body_html("## Moving parts!\n\n```rovik run in=part:Lava\nif a < b then end\n```\n");
        assert_eq!(heads, [("moving-parts".to_string(), "Moving parts!".to_string())]);
        assert!(html.contains("id=\"moving-parts\""));
        assert!(html.contains("<pre class=\"code rovik\"><code>if a &lt; b then end</code></pre>"));
        assert!(!html.contains("run in="), "harness notes aren't shown");
    }

    #[test]
    fn pictures_become_figures_with_captions() {
        let (html, _) = body_html("![The **Explorer** panel](img/x.png)\n");
        assert!(html.contains("<figure><img src=\"img/x.png\" alt=\"The Explorer panel\""), "{html}");
        assert!(html.contains("<figcaption>The Explorer panel</figcaption>"));
    }
}
