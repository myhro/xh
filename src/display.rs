use std::fmt::Write;

use crate::Pretty;
use ansi_term;
use ansi_term::Color::{self, Fixed, RGB};
use reqwest::blocking::{Request, Response};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::Serialize;
use serde_json::Value;
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::{SyntaxSet, SyntaxSetBuilder};
use syntect::util::LinesWithEndings;

// https://github.com/sharkdp/bat/blob/3a85fd767bd1f03debd0a60ac5bc08548f95bc9d/src/terminal.rs
fn to_ansi_color(color: syntect::highlighting::Color) -> ansi_term::Color {
    if color.a == 0 {
        // Themes can specify one of the user-configurable terminal colors by
        // encoding them as #RRGGBBAA with AA set to 00 (transparent) and RR set
        // to the 8-bit color palette number. The built-in themes ansi-light,
        // ansi-dark, base16, and base16-256 use this.
        match color.r {
            // For the first 8 colors, use the Color enum to produce ANSI escape
            // sequences using codes 30-37 (foreground) and 40-47 (background).
            // For example, red foreground is \x1b[31m. This works on terminals
            // without 256-color support.
            0x00 => Color::Black,
            0x01 => Color::Red,
            0x02 => Color::Green,
            0x03 => Color::Yellow,
            0x04 => Color::Blue,
            0x05 => Color::Purple,
            0x06 => Color::Cyan,
            0x07 => Color::White,
            // For all other colors, use Fixed to produce escape sequences using
            // codes 38;5 (foreground) and 48;5 (background). For example,
            // bright red foreground is \x1b[38;5;9m. This only works on
            // terminals with 256-color support.
            //
            // TODO: When ansi_term adds support for bright variants using codes
            // 90-97 (foreground) and 100-107 (background), we should use those
            // for values 0x08 to 0x0f and only use Fixed for 0x10 to 0xff.
            n => Fixed(n),
        }
    } else {
        RGB(color.r, color.g, color.b)
    }
}

fn colorize<'a>(text: &'a str, syntax: &str) -> impl Iterator<Item = String> + 'a {
    lazy_static! {
        static ref TS: ThemeSet = ThemeSet::load_from_folder("assets").unwrap();
        static ref PS: SyntaxSet = {
            let mut ps = SyntaxSetBuilder::new();
            ps.add_from_folder("assets", true).unwrap();
            ps.build()
        };
    }
    let syntax = PS.find_syntax_by_extension(syntax).unwrap();
    let mut h = HighlightLines::new(syntax, &TS.themes["ansi"]);
    LinesWithEndings::from(text).map(move |line| {
        let mut s: String = String::new();
        let highlights = h.highlight(line, &PS);
        for (style, component) in highlights {
            let color = to_ansi_color(style.foreground);
            write!(s, "{}", &color.paint(component)).unwrap();
        }
        s
    })
}

fn indent_json(text: &str) -> String {
    let data: Value = serde_json::from_str(&text).unwrap();
    let buf = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(b"    ");
    let mut ser = serde_json::Serializer::with_formatter(buf, formatter);
    data.serialize(&mut ser).unwrap();
    String::from_utf8(ser.into_inner()).unwrap()
}

pub fn print_json(text: &str, options: &Pretty) {
    match options {
        Pretty::All => {
            colorize(&indent_json(text), "json").for_each(|line| print!("{}", line));
        }
        Pretty::Colors => {
            colorize(text, "json").for_each(|line| print!("{}", line));
        }
        Pretty::Format => println!("{}", indent_json(text)),
        Pretty::None => println!("{}", text),
    }
    println!("\x1b[0m");
}

pub fn print_xml(text: &str, options: &Pretty) {
    match options {
        Pretty::All | Pretty::Colors => colorize(text, "xml").for_each(|line| print!("{}", line)),
        Pretty::Format | Pretty::None => println!("{}", text),
    }
    println!("\x1b[0m");
}

pub fn print_html(text: &str, options: &Pretty) {
    match options {
        Pretty::All | Pretty::Colors => colorize(text, "html").for_each(|line| print!("{}", line)),
        Pretty::Format | Pretty::None => println!("{}", text),
    }
    println!("\x1b[0m");
}

fn headers_to_string(headers: &HeaderMap, sort: bool) -> String {
    let mut headers: Vec<(&HeaderName, &HeaderValue)> = headers.iter().collect();
    if sort {
        headers.sort_by(|(a, _), (b, _)| a.to_string().cmp(&b.to_string()))
    }

    let mut header_string = String::new();
    for (key, value) in headers {
        let key = key.to_string();
        let value = value.to_str().unwrap();
        writeln!(&mut header_string, "{}: {}", key, value).unwrap();
    }

    header_string
}

pub fn print_request_headers(request: &Request) {
    let method = request.method();
    let url = request.url();
    let query_string = url.query().map_or(String::from(""), |q| ["?", q].concat());
    let version = reqwest::Version::HTTP_11;
    let headers = request.headers();

    let request_line = format!("{} {}{} {:?}\n", method, url.path(), query_string, version);
    let headers = &headers_to_string(headers, true);

    for line in colorize(&(request_line + &headers), "http") {
        print!("{}", line)
    }
    println!("\x1b[0m");
}

pub fn print_response_headers(response: &Response) {
    let version = response.version();
    let status = response.status();
    let headers = response.headers();

    let status_line = format!(
        "{:?} {} {}\n",
        version,
        status.as_str(),
        status.canonical_reason().unwrap()
    );
    let headers = headers_to_string(headers, true);

    for line in colorize(&(status_line + &headers), "http") {
        print!("{}", line)
    }
    println!("\x1b[0m");
}

// TODO: support pretty printing more response types
pub fn print_body(body: Box<dyn FnOnce() -> String>, content_type: Option<&str>, pretty: &Pretty) {
    if let Some(content_type) = content_type {
        if !content_type.contains("application") && !content_type.contains("text") {
            print!("\n\n");
            println!("+-----------------------------------------+");
            println!("| NOTE: binary data not shown in terminal |");
            println!("+-----------------------------------------+");
            print!("\n\n");
        } else if content_type.contains("json") {
            print_json(&body(), &pretty);
        } else if content_type.contains("xml") {
            print_xml(&body(), &pretty);
        } else if content_type.contains("html") {
            print_html(&body(), &pretty);
        } else {
            println!("{}", &body());
        }
    }
    print!("\n");
}