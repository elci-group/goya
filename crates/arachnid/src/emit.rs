//! HTML, CSS and JavaScript emission (Web) and the remote runtime (Tarsus).
//!
//! The ontology is laid out at compile time at exactly 1920×1080, so the page
//! needs no layout engine. Every node becomes an absolutely positioned `div`
//! at its resolved rectangle, in draw order: parents before children, with
//! later nodes on top. One CSS pixel is one point at the app's fixed
//! 1920×1080 resolution.
//!
//! The generated JavaScript is ES5 and uses no web feature newer than
//! Chromium 38. It therefore runs on every supported engine without
//! transpiling, and [`REQUIRED_FEATURES`] is empty. If the emitter ever
//! uses a newer feature, that feature must be added to the list; the build
//! then refuses targets whose engine lacks it (`GY-AR-660`).
//!
//! The runtime does the following:
//! * **5-way.** Keys 37–40 move focus along the compile-time graph. Key 13
//!   (OK) activates the focused element.
//! * **Pointer.** `mouseover` moves focus to the element under the Magic
//!   Remote cursor, and `click` activates it.
//! * **Modality.** Pointer mode turns on with mouse movement or a visible
//!   `cursorStateChange`, and turns off with an arrow key or a hidden
//!   `cursorStateChange`. In pointer mode, OK is left to the platform's
//!   click. A click during an OK press that already activated by key is
//!   ignored. One press gives exactly one activation in either mode.
//! * **Activation.** Activation dispatches `goya:activate` with the element's
//!   authored id. No `@interaction` is lowered yet (see `ontology.rs`), so
//!   nothing listens to it so far.
//! * **Back.** Key 461 is left to the platform (`disableBackHistoryAPI` is
//!   false).
//!
//! Whether pointer-mode OK delivers `click`, `keydown 13` or both has **not**
//! been observed on a TV. The rules above give one activation under each of
//! those cases, apart from pointer-mode OK with no click event. Qualifying
//! this needs the owner's TV.

use crate::appinfo::{json_string, AppInfo};
use crate::focus::Direction;
use crate::ontology::{Ontology, ONTOLOGY_HEIGHT_PT, ONTOLOGY_WIDTH_PT};
use goya_caprichios::webos_engine_caprichio::WebFeature;
use goya_lang::Anchor;

/// Web features the emitted code depends on. Empty: the output is ES5 with
/// CSS2.1 positioning plus `box-shadow`.
pub const REQUIRED_FEATURES: &[WebFeature] = &[];

/// Millipoints → CSS px text, exact to three decimals with trailing zeros
/// trimmed. Deterministic.
pub fn px(mp: i64) -> String {
    let sign = if mp < 0 { "-" } else { "" };
    let abs = mp.unsigned_abs();
    let (whole, frac) = (abs / 1000, abs % 1000);
    if frac == 0 {
        format!("{sign}{whole}px")
    } else {
        let f = format!("{frac:03}");
        format!("{sign}{whole}.{}px", f.trim_end_matches('0'))
    }
}

fn colour(c: u32) -> String {
    format!("#{:06x}", c & 0x00FF_FFFF)
}

pub fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

pub fn index_html(o: &Ontology, app: &AppInfo) -> String {
    let focus_of = |node: usize| {
        o.actionable
            .iter()
            .find(|a| a.node == node)
            .map(|a| a.focus_id)
    };
    let mut body = String::new();
    for (i, node) in o.frame.nodes.iter().enumerate() {
        let r = node.rect;
        let mut style = format!(
            "left:{};top:{};width:{};height:{}",
            px(r.x),
            px(r.y),
            px(r.width),
            px(r.height)
        );
        if let Some(fill) = node.fill {
            style.push_str(&format!(";background:{}", colour(fill)));
        }
        if let (Some(stroke), Some(w)) = (node.stroke, node.stroke_width_mp) {
            style.push_str(&format!(
                ";border:{} solid {}",
                px(w.max(0)),
                colour(stroke)
            ));
        }
        if let Some(size) = node.text_size_mp {
            style.push_str(&format!(";font-size:{}", px(size.max(0))));
        }
        match node.anchor {
            Anchor::Start => {}
            Anchor::Middle => style.push_str(";text-align:center"),
            Anchor::End => style.push_str(";text-align:right"),
        }
        let mut attrs = format!("id=\"n{i}\" class=\"n");
        if let Some(fid) = focus_of(i) {
            attrs.push_str(&format!(" b\" role=\"button\" data-focus=\"{fid}\""));
        } else {
            attrs.push('"');
        }
        if let Some(id) = &o.authored_ids[i] {
            attrs.push_str(&format!(" data-goya-id=\"{}\"", html_escape(id)));
        }
        let text = node.text.as_deref().map(html_escape).unwrap_or_default();
        body.push_str(&format!("<div {attrs} style=\"{style}\">{text}</div>\n"));
    }
    format!(
        "<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n<title>{}</title>\n<link rel=\"stylesheet\" href=\"app.css\">\n</head>\n<body>\n<div id=\"ontology\">\n{body}</div>\n<script src=\"app.js\"></script>\n</body>\n</html>\n",
        html_escape(&app.title)
    )
}

pub fn app_css() -> String {
    format!(
        "html,body{{margin:0;padding:0;width:{w}px;height:{h}px;overflow:hidden;background:#000}}\n\
         #ontology{{position:relative;width:{w}px;height:{h}px;overflow:hidden;background:#fff;font-family:sans-serif;color:#000}}\n\
         .n{{position:absolute;box-sizing:border-box;overflow:hidden;white-space:pre;margin:0;padding:0}}\n\
         .b{{cursor:pointer;text-align:center}}\n\
         .b.f{{box-shadow:0 0 0 4px #000,0 0 0 8px #fff;z-index:1}}\n",
        w = ONTOLOGY_WIDTH_PT,
        h = ONTOLOGY_HEIGHT_PT
    )
}

/// Focus table as JS: `{"0":[up,down,left,right],…}` with `null` gaps.
fn graph_js(o: &Ontology) -> String {
    let rows: Vec<String> = o
        .graph
        .iter()
        .map(|row| {
            let cells: Vec<String> = Direction::ALL
                .iter()
                .map(|d| row.successors[d.slot()].map_or("null".into(), |n| n.to_string()))
                .collect();
            format!("\"{}\":[{}]", row.id, cells.join(","))
        })
        .collect();
    format!("{{{}}}", rows.join(","))
}

pub fn app_js(o: &Ontology) -> String {
    let initial = o.initial.map_or("null".to_string(), |i| i.to_string());
    format!(
        r#"/* Generated by Arachnid ({contract}). ES5; no web feature newer than Chromium 38. */
(function () {{
  "use strict";
  var GRAPH = {graph};
  var INITIAL = {initial};
  var SLOT = {{ 38: 0, 40: 1, 37: 2, 39: 3 }};
  var nodes = {{}};
  var current = null;
  var pointerMode = false;
  var okHeld = false;
  var keyActivatedThisPress = false;

  function focus(id) {{
    if (id === null || !nodes[id] || id === current) return;
    if (current !== null && nodes[current]) nodes[current].className = "n b";
    current = id;
    nodes[id].className = "n b f";
  }}

  function activate(id, via) {{
    var el = nodes[id];
    if (!el) return;
    var ev = document.createEvent("CustomEvent");
    ev.initCustomEvent("goya:activate", false, false, {{ id: el.getAttribute("data-goya-id"), via: via }});
    document.dispatchEvent(ev);
  }}

  document.addEventListener("keydown", function (e) {{
    var k = e.keyCode;
    if (SLOT.hasOwnProperty(k)) {{
      e.preventDefault();
      pointerMode = false;
      if (current === null) {{ focus(INITIAL); return; }}
      var next = GRAPH[current] ? GRAPH[current][SLOT[k]] : null;
      if (next !== null) focus(next);
      return;
    }}
    if (k === 13) {{
      if (okHeld) {{ e.preventDefault(); return; }}
      okHeld = true;
      keyActivatedThisPress = false;
      if (pointerMode || current === null) return;
      e.preventDefault();
      keyActivatedThisPress = true;
      activate(current, "key");
    }}
  }});
  document.addEventListener("keyup", function (e) {{
    if (e.keyCode === 13) okHeld = false;
  }});
  document.addEventListener("cursorStateChange", function (e) {{
    pointerMode = !!(e.detail && e.detail.visibility);
  }});
  document.addEventListener("mousemove", function () {{ pointerMode = true; }});
  window.addEventListener("blur", function () {{ okHeld = false; }});

  function wire(el, id) {{
    nodes[id] = el;
    el.addEventListener("mouseover", function () {{ pointerMode = true; focus(id); }});
    el.addEventListener("click", function (e) {{
      e.preventDefault();
      if (okHeld && keyActivatedThisPress) return;
      pointerMode = true;
      focus(id);
      activate(id, "pointer");
    }});
  }}

  var all = document.querySelectorAll("[data-focus]");
  for (var i = 0; i < all.length; i++) {{
    wire(all[i], parseInt(all[i].getAttribute("data-focus"), 10));
  }}
  focus(INITIAL);
}})();
"#,
        contract = crate::CONTRACT,
        graph = graph_js(o),
        initial = initial
    )
}

/// `navigation-graph.json`: every actionable element with its rectangle
/// and successors, using authored ids.
pub fn navigation_report(o: &Ontology) -> String {
    let label = |id: u32| {
        let a = &o.actionable[id as usize];
        json_string(a.authored_id.as_deref().unwrap_or(&a.name))
    };
    let rows: Vec<String> = o
        .graph
        .iter()
        .map(|row| {
            let a = &o.actionable[row.id as usize];
            let r = o.frame.nodes[a.node].rect;
            let succ: Vec<String> = Direction::ALL
                .iter()
                .map(|d| {
                    let name = match d {
                        Direction::Up => "up",
                        Direction::Down => "down",
                        Direction::Left => "left",
                        Direction::Right => "right",
                    };
                    format!(
                        "\"{name}\": {}",
                        row.successors[d.slot()].map_or("null".into(), label)
                    )
                })
                .collect();
            format!(
                "    {{\"focus\": {}, \"id\": {}, \"rect_mp\": [{}, {}, {}, {}], {}}}",
                row.id,
                label(row.id),
                r.x,
                r.y,
                r.width,
                r.height,
                succ.join(", ")
            )
        })
        .collect();
    format!(
        "{{\n  \"contract\": \"{}\",\n  \"ontology_limit_pt\": [{}, {}],\n  \"initial\": {},\n  \"unreachable\": [],\n  \"traps\": [],\n  \"elements\": [\n{}\n  ]\n}}\n",
        crate::focus::CONTRACT,
        ONTOLOGY_WIDTH_PT,
        ONTOLOGY_HEIGHT_PT,
        o.initial.map_or("null".into(), label),
        rows.join(",\n")
    )
}

/// Default 80×80 launcher icon: a dark tile with a light square in the
/// middle. Deterministic.
pub fn default_icon() -> Vec<u8> {
    let mut rgb = Vec::with_capacity(80 * 80 * 3);
    for y in 0..80 {
        for x in 0..80 {
            let inner = (24..56).contains(&x) && (24..56).contains(&y);
            rgb.extend_from_slice(if inner {
                &[0xE8, 0xEC, 0xF0]
            } else {
                &[0x1F, 0x2A, 0x36]
            });
        }
    }
    crate::archive::png_rgb(80, 80, &rgb)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn px_is_exact() {
        assert_eq!(px(96_000), "96px");
        assert_eq!(px(12_345), "12.345px");
        assert_eq!(px(12_300), "12.3px");
        assert_eq!(px(5), "0.005px");
        assert_eq!(px(0), "0px");
    }

    #[test]
    fn escaping() {
        assert_eq!(
            html_escape("<a href='x'>&\"</a>"),
            "&lt;a href=&#39;x&#39;&gt;&amp;&quot;&lt;/a&gt;"
        );
    }

    #[test]
    fn icon_is_80_square_png() {
        assert_eq!(
            crate::archive::png_dimensions(&default_icon()),
            Some((80, 80))
        );
    }
}
