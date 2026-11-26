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

pub fn index_html(o: &Ontology, app: &AppInfo, host_script: Option<&str>) -> String {
    let focus_of = |node: usize| {
        o.actionable
            .iter()
            .find(|a| a.node == node)
            .map(|a| a.focus_id)
    };
    let mut body = String::new();
    // Ends of open drift wrappers, innermost last.
    let mut open: Vec<usize> = Vec::new();
    for (i, node) in o.frame.nodes.iter().enumerate() {
        if let Some(d) = o.motion.drift[i] {
            body.push_str(&format!(
                "<div class=\"m{}\" style=\"--ax:{};--ay:{};animation-duration:{}ms\">\n",
                if d.decorative { " dec" } else { "" },
                px(d.ax),
                px(d.ay),
                d.period_ms
            ));
            open.push(o.motion.subtree_end[i]);
        }
        let initial_nodes = &o.configurations[o.initial_configuration].present_nodes;
        if let Some(a) = o.motion.appear[i] {
            // Fades (and slides) in at launch when present, and again each
            // time the runtime sees the subtree become present.
            body.push_str(&format!(
                "<div class=\"w{}\" data-appear=\"{i}\" style=\"--dx:{};--dy:{};animation-duration:{}ms\">\n",
                if initial_nodes[i] { " a" } else { "" },
                px(a.dx),
                px(a.dy),
                a.ms
            ));
            open.push(o.motion.subtree_end[i]);
        }
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
        if o.text_from[i].is_some() {
            style.push_str(";white-space:pre-wrap");
        }
        let flow = o.motion.flow[i];
        let mut attrs = format!("id=\"n{i}\" class=\"n{}", if flow.is_some() { " z" } else { "" });
        if let Some(fid) = focus_of(i) {
            attrs.push_str(&format!(" b\" role=\"button\" data-focus=\"{fid}\""));
        } else {
            attrs.push('"');
        }
        if let Some(digit) = o.remote_digit[i] {
            attrs.push_str(&format!(" data-remote-digit=\"{digit}\""));
        }
        if let Some(id) = &o.authored_ids[i] {
            attrs.push_str(&format!(" data-goya-id=\"{}\"", html_escape(id)));
        }
        if !initial_nodes[i] {
            attrs.push_str(" hidden");
        }
        let mut text = String::new();
        if let Some(f) = flow {
            // A cross-fade layer under the text: a gradient from fill-to to
            // fill, faded in and out. Every colour lies on the segment the
            // contrast check samples.
            text.push_str(&format!(
                "<div class=\"g\" style=\"background:linear-gradient(115deg,{},{});animation-duration:{}ms\"></div>",
                colour(f.to),
                colour(f.from),
                f.period_ms
            ));
        }
        text.push_str(&node.text.as_deref().map(html_escape).unwrap_or_default());
        if let Some(key) = &o.text_from[i] {
            // The runtime replaces only this span, never the node's layers.
            let initial = o
                .state
                .iter()
                .find(|k| &k.key == key)
                .map(|k| html_escape(&display(&k.initial)))
                .unwrap_or_default();
            text.push_str(&format!(
                "<span data-text-from=\"{}\">{initial}</span>",
                html_escape(key)
            ));
        }
        if let Some(key) = &o.image_from[i] {
            text.push_str(&format!(
                "<img class=\"i\" alt=\"\" hidden data-image-from=\"{}\">",
                html_escape(key)
            ));
            text.push_str(&format!("<video class=\"i\" controls playsinline preload=\"metadata\" hidden data-image-from=\"{}\" aria-label=\"Video review playback\"></video>", html_escape(key)));
        }
        body.push_str(&format!("<div {attrs} style=\"{style}\">{text}</div>\n"));
        while open.last() == Some(&(i + 1)) {
            open.pop();
            body.push_str("</div>\n");
        }
    }
    debug_assert!(open.is_empty(), "every drift wrapper closes inside the frame");
    format!(
        "<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n<title>{}</title>\n<link rel=\"stylesheet\" href=\"app.css\">\n</head>\n<body>\n<div id=\"ontology\">\n{body}{}</div>\n<script src=\"app.js\"></script>\n{}</body>\n</html>\n",
        html_escape(&app.title),
        crate::splash::markup(),
        host_script.map_or(String::new(), |h| format!("<script src=\"{}\"></script>\n", html_escape(h)))
    )
}

pub fn app_css() -> String {
    format!(
        "html,body{{margin:0;padding:0;width:{w}px;height:{h}px;overflow:hidden;background:#000}}\n\
         #ontology{{position:relative;width:{w}px;height:{h}px;overflow:hidden;background:#fff;font-family:sans-serif;color:#000}}\n\
         .n{{position:absolute;box-sizing:border-box;overflow:hidden;white-space:pre;margin:0;padding:0}}\n\
         .b{{cursor:pointer;text-align:center;display:flex;align-items:center;justify-content:center}}\n\
         .b[hidden]{{display:none}}\n\
         .b.f{{box-shadow:0 0 0 4px #000,0 0 0 8px #fff;z-index:1}}\n\
         .i{{position:absolute;left:0;top:0;width:100%;height:100%;object-fit:contain}}\n\
         .n.z{{z-index:0}}\n\
         .g{{position:absolute;left:0;top:0;width:100%;height:100%;z-index:-1;opacity:0;pointer-events:none;will-change:opacity;animation:breathe 1s ease-in-out infinite alternate}}\n\
         @keyframes breathe{{from{{opacity:0}}to{{opacity:1}}}}\n\
         .m{{position:absolute;left:0;top:0;width:{w}px;height:{h}px;pointer-events:none;will-change:transform;animation:orbit 1s linear infinite}}\n\
         .m .n,.w .n{{pointer-events:auto}}\n\
         .w{{position:absolute;left:0;top:0;width:{w}px;height:{h}px;pointer-events:none}}\n\
         .w.a{{animation:appear 1s ease-out both}}\n\
         @keyframes appear{{from{{opacity:0;transform:translate(var(--dx),var(--dy))}}to{{opacity:1;transform:none}}}}\n\
         @keyframes fadein{{from{{opacity:0}}to{{opacity:1}}}}\n\
         @keyframes orbit{{\
0%,100%{{transform:translate(var(--ax),0)}}\
12.5%{{transform:translate(calc(var(--ax)*.7071),calc(var(--ay)*.7071))}}\
25%{{transform:translate(0,var(--ay))}}\
37.5%{{transform:translate(calc(var(--ax)*-.7071),calc(var(--ay)*.7071))}}\
50%{{transform:translate(calc(var(--ax)*-1),0)}}\
62.5%{{transform:translate(calc(var(--ax)*-.7071),calc(var(--ay)*-.7071))}}\
75%{{transform:translate(0,calc(var(--ay)*-1))}}\
87.5%{{transform:translate(calc(var(--ax)*.7071),calc(var(--ay)*-.7071))}}}}\n\
         @media (prefers-reduced-motion:reduce){{.g,.m.dec{{animation:none}}.w.a{{animation-name:fadein}}}}\n{splash}",
        w = ONTOLOGY_WIDTH_PT,
        h = ONTOLOGY_HEIGHT_PT,
        splash = crate::splash::css()
    )
}

/// Display text of a state value, as `text-from` shows it.
pub fn display(v: &goya_lang::InteractionValue) -> String {
    match v {
        goya_lang::InteractionValue::Int(i) => i.to_string(),
        goya_lang::InteractionValue::Text(t) => t.clone(),
        goya_lang::InteractionValue::Bool(b) => b.to_string(),
    }
}

/// Focus table as JS: `{"0":[up,down,left,right],…}` with `null` gaps.
fn graph_js(graph: &[crate::focus::FocusSuccessors]) -> String {
    let rows: Vec<String> = graph
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

fn type_name(t: goya_lang::InteractionType) -> &'static str {
    match t {
        goya_lang::InteractionType::Int => "int",
        goya_lang::InteractionType::Text => "text",
        goya_lang::InteractionType::Bool => "bool",
    }
}

fn result_name(t: goya_lang::EffectValueType) -> &'static str {
    match t {
        goya_lang::EffectValueType::Int64 => "int",
        goya_lang::EffectValueType::Text => "text",
        goya_lang::EffectValueType::Bool => "bool",
    }
}

/// The runtime's data tables, as JS declarations.
fn tables_js(o: &Ontology) -> String {
    use crate::live::js_value;
    let js = crate::appinfo::json_string;
    // Per node, the full conjunction: [key, "is"|"ge", literal].
    let conds: Vec<String> = o
        .presence
        .node_conditions
        .iter()
        .enumerate()
        .filter(|(_, c)| !c.is_empty())
        .map(|(i, c)| {
            let parts: Vec<String> = c
                .iter()
                .map(|c| {
                    let op = match c.op {
                        goya_lang::ConditionOp::Is => "is",
                        goya_lang::ConditionOp::AtLeast => "ge",
                    };
                    format!("[{},\"{op}\",{}]", js(&o.presence.keys[c.key].key), js_value(&c.value))
                })
                .collect();
            format!("\"{i}\":[{}]", parts.join(","))
        })
        .collect();
    // Focus graphs keyed by the set of present buttons; configurations that
    // differ only in undrawn or non-button nodes share one entry.
    let mut seen = std::collections::BTreeSet::new();
    let configs: Vec<String> = o
        .configurations
        .iter()
        .filter(|c| seen.insert(c.present.clone()))
        .map(|c| {
            let key: Vec<String> = c.present.iter().map(|i| i.to_string()).collect();
            format!(
                "\"{}\":{{graph:{},initial:{}}}",
                key.join(","),
                graph_js(&c.graph),
                c.initial.map_or("null".into(), |i| i.to_string())
            )
        })
        .collect();
    let node_of: Vec<String> = o.actionable.iter().map(|a| a.node.to_string()).collect();
    let state: Vec<String> = o
        .state
        .iter()
        .map(|k| format!("{}:{}", js(&k.key), js_value(&k.initial)))
        .collect();
    let types: Vec<String> = o
        .state
        .iter()
        .map(|k| format!("{}:\"{}\"", js(&k.key), type_name(k.value_type)))
        .collect();
    let host: Vec<String> = o
        .state
        .iter()
        .filter(|k| k.host)
        .map(|k| format!("{}:true", js(&k.key)))
        .collect();
    let interactions: Vec<String> = o
        .interactions
        .iter()
        .filter_map(|l| {
            let a = o.actionable.iter().find(|a| a.name == l.trigger_button)?;
            let requests: Vec<String> = l
                .requests
                .iter()
                .map(|(name, label, request, result)| {
                    format!(
                        "[{},{},{},\"{}\"]",
                        js(name),
                        js(label),
                        js(request).replace('<', "\\u003c"),
                        result_name(*result)
                    )
                })
                .collect();
            let writes: Vec<String> = l
                .writes
                .iter()
                .map(|(k, e)| format!("w[{}] = {e};", js(k)))
                .collect();
            Some(format!(
                "\"{}\":{{name:{},d2:{},requests:[{}],run:function (s, e) {{ var w = {{}}; {} return w; }}}}",
                a.focus_id,
                js(&l.name),
                l.d2,
                requests.join(","),
                writes.join(" ")
            ))
        })
        .collect();
    format!(
        "  var NODE_COND = {{{}}};\n  var CONFIGS = {{{}}};\n  var NODE_OF = [{}];\n  var STATE = {{{}}};\n  var TYPES = {{{}}};\n  var HOST = {{{}}};\n  var INTERACTIONS = {{{}}};\n",
        conds.join(","),
        configs.join(","),
        node_of.join(","),
        state.join(","),
        types.join(","),
        host.join(","),
        interactions.join(",\n    ")
    )
}

pub fn app_js(o: &Ontology) -> String {
    format!(
        r#"/* Generated by Arachnid ({contract}, {live}). ES5; D2 effects also use Promise. */
(function () {{
  "use strict";
{tables}  var SLOT = {{ 38: 0, 40: 1, 37: 2, 39: 3 }};
  var MAX = 9007199254740991;
  var nodes = {{}};
  var current = null;
  var sig = null;
  var lastFocus = {{}};
  var pending = {{}};
  var providers = {{}};
  var imageUrls = {{}};
  var errors = [];
  var appearEls = [];
  var wasPresent = {{}};
{splash}
  /* Pointer mode is entered only when the platform reports a visible
     cursor. A stock remote sends no pointer events, so a stray mouse event
     must never make OK wait for a click that will not come. */
  var pointerMode = false;
  var okHeld = false;
  var keyActivatedThisPress = false;

  function fail(where, err) {{
    var msg = where + ": " + (err && err.message ? err.message : String(err));
    errors.push(msg);
    if (errors.length > 50) errors.shift();
    if (window.console) console.error("arachnid " + msg);
  }}

  /* Forget a press that was cut short: the app was blurred, hidden or
     relaunched while OK was down, so its keyup never arrived. */
  function resetPress() {{
    okHeld = false;
    keyActivatedThisPress = false;
  }}

  /* Checked addition with the reference VM's semantics, limited to the
     range JavaScript represents exactly. */
  function add(a, b) {{
    var r = a + b;
    if (typeof r !== "number" || r % 1 !== 0 || r > MAX || r < -MAX) throw new Error("integer overflow");
    return r;
  }}

  function typed(key, v) {{
    var t = TYPES[key];
    if (t === "int") return typeof v === "number" && v % 1 === 0 && v <= MAX && v >= -MAX;
    if (t === "text") return typeof v === "string";
    if (t === "bool") return typeof v === "boolean";
    return false;
  }}

  function presentNode(i) {{
    var c = NODE_COND[i];
    if (!c) return true;
    for (var j = 0; j < c.length; j++) {{
      var v = STATE[c[j][0]];
      if (c[j][1] === "ge" ? !(typeof v === "number" && v >= c[j][2]) : v !== c[j][2]) return false;
    }}
    return true;
  }}

  /* The configuration key: the set of present buttons. */
  function signature() {{
    var p = [];
    for (var i = 0; i < NODE_OF.length; i++) if (presentNode(NODE_OF[i])) p.push(i);
    return p.join(",");
  }}

  function present(id) {{ return id !== null && nodes[id] && presentNode(NODE_OF[id]); }}

  function focus(id) {{
    if (id === null || !present(id) || id === current) return;
    if (current !== null && nodes[current]) nodes[current].className = "n b";
    current = id;
    lastFocus[sig] = id;
    nodes[id].className = "n b f";
  }}

  function clearFocus() {{
    if (current !== null && nodes[current]) nodes[current].className = "n b";
    current = null;
  }}

  /* After a configuration change: keep focus if its element is still
     present, else return to where focus last was in this configuration,
     else take the configuration's initial focus. */
  function refocus() {{
    if (present(current)) {{ lastFocus[sig] = current; return; }}
    var back = lastFocus[sig];
    clearFocus();
    if (back !== undefined && present(back)) focus(back);
    else focus(CONFIGS[sig] ? CONFIGS[sig].initial : null);
  }}

  function render() {{
    var i, el, list = document.querySelectorAll("[data-text-from]");
    for (i = 0; i < list.length; i++) {{
      el = list[i];
      el.textContent = String(STATE[el.getAttribute("data-text-from")]);
    }}
    for (var n in NODE_COND) {{
      if (NODE_COND.hasOwnProperty(n)) {{
        el = document.getElementById("n" + n);
        if (el) el.hidden = !presentNode(parseInt(n, 10));
      }}
    }}
    /* Replay appear animations for subtrees that just became present. */
    for (i = 0; i < appearEls.length; i++) {{
      var w = appearEls[i], an = parseInt(w.getAttribute("data-appear"), 10), now = presentNode(an);
      if (now && !wasPresent[an]) {{
        w.className = "w";
        void w.offsetWidth;
        w.className = "w a";
      }}
      wasPresent[an] = now;
    }}
    var next = signature();
    if (next !== sig) {{ sig = next; resetPress(); refocus(); }}
  }}

  function commit(writes) {{
    for (var k in writes) {{
      if (writes.hasOwnProperty(k) && !typed(k, writes[k])) throw new Error("write to " + k + " has the wrong type");
    }}
    for (k in writes) if (writes.hasOwnProperty(k)) STATE[k] = writes[k];
    render();
  }}

  function snapshot() {{
    var s = {{}};
    for (var k in STATE) if (STATE.hasOwnProperty(k)) s[k] = STATE[k];
    return s;
  }}

  function runInteraction(id) {{
    var it = INTERACTIONS[id];
    if (!it || pending[id]) return;
    if (!it.d2) {{
      try {{ commit(it.run(snapshot(), {{}})); }} catch (err) {{ fail(it.name, err); }}
      return;
    }}
    /* D2: serve each request through the host in declared order, then
       commit atomically. A failure or wrongly typed result commits
       nothing. One activation at a time per interaction. */
    pending[id] = true;
    var pre = snapshot(), results = {{}}, i = 0;
    function next() {{
      if (i >= it.requests.length) return Promise.resolve();
      var r = it.requests[i++], fn = providers[r[1]];
      if (!fn) return Promise.reject(new Error("no host provides " + r[1]));
      return Promise.resolve(fn(r[2])).then(function (v) {{
        var ok = r[3] === "int" ? typeof v === "number" && v % 1 === 0 && v <= MAX && v >= -MAX
          : r[3] === "text" ? typeof v === "string" : typeof v === "boolean";
        if (!ok) throw new Error(r[1] + " returned the wrong type");
        results[r[0]] = v;
        return next();
      }});
    }}
    next().then(function () {{ commit(it.run(pre, results)); }})
      ["catch"](function (err) {{ fail(it.name, err); }})
      .then(function () {{ pending[id] = false; }});
  }}

  function activate(id) {{
    var el = nodes[id];
    if (!el || !present(id)) return;
    var ev = document.createEvent("CustomEvent");
    ev.initCustomEvent("goya:activate", false, false, {{ id: el.getAttribute("data-goya-id"), via: arguments[1] }});
    document.dispatchEvent(ev);
    runInteraction(id);
  }}

  document.addEventListener("keydown", function (e) {{
    var k = e.keyCode;
    /* The splash takes the 5-way keys; everything else reaches the platform. */
    if (splashOn && (SLOT.hasOwnProperty(k) || k === 13)) {{ e.preventDefault(); return; }}
    // Publish a semantic digit without assigning it to a particular app.
    // Keep keyboard shortcuts, text entry and held keys out of this channel.
    var target = e.target;
    var editing = target && (target.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(target.tagName));
    if (!splashOn && !editing && !e.defaultPrevented && !e.repeat &&
        !e.altKey && !e.ctrlKey && !e.metaKey && !e.shiftKey) {{
      var digit = typeof e.key === "string" && /^[0-9]$/.test(e.key) ? Number(e.key) : null;
      if (digit === null && (!e.key || e.key === "Unidentified") && k >= 48 && k <= 57) digit = k - 48;
      if (digit !== null) {{
        var numberEvent = document.createEvent("CustomEvent");
        numberEvent.initCustomEvent("goya:digit", false, true, {{ digit: digit }});
        if (!document.dispatchEvent(numberEvent)) {{
          e.preventDefault();
          return;
        }}
        var buttons = document.querySelectorAll("[data-remote-digit]");
        for (var i = 0; i < buttons.length; i++) {{
          if (Number(buttons[i].getAttribute("data-remote-digit")) !== digit) continue;
          var id = Number(buttons[i].getAttribute("data-focus"));
          if (!present(id)) continue;
          e.preventDefault();
          pointerMode = false;
          focus(id);
          activate(id, "digit");
          break;
        }}
        return;
      }}
    }}
    if (SLOT.hasOwnProperty(k)) {{
      e.preventDefault();
      pointerMode = false;
      if (current === null) {{ focus(CONFIGS[sig] ? CONFIGS[sig].initial : null); return; }}
      var graph = CONFIGS[sig] ? CONFIGS[sig].graph : {{}};
      var next = graph[current] ? graph[current][SLOT[k]] : null;
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
  window.addEventListener("blur", resetPress);
  document.addEventListener("visibilitychange", resetPress);
  document.addEventListener("webOSRelaunch", resetPress);

  function wire(el, id) {{
    nodes[id] = el;
    el.addEventListener("mouseover", function () {{ focus(id); }});
    el.addEventListener("click", function (e) {{
      e.preventDefault();
      if (splashOn) return;
      if (okHeld && keyActivatedThisPress) return;
      focus(id);
      activate(id, "pointer");
    }});
  }}

  /* The host API. Host scripts load after this one. */
  window.Arachnid = {{
    contract: "{live}",
    get: function (key) {{ return STATE[key]; }},
    set: function (key, value) {{
      if (!HOST[key]) throw new Error(key + " is not host state");
      if (!typed(key, value)) throw new Error(key + " needs a " + TYPES[key]);
      STATE[key] = value;
      render();
    }},
    provide: function (label, fn) {{
      if (typeof fn !== "function") throw new Error("provider for " + label + " must be a function");
      providers[label] = fn;
    }},
    setImage: function (key, blob, handle) {{
      if (!HOST[key] || TYPES[key] !== "text") throw new Error(key + " is not a host text key");
      if (!(blob instanceof Blob) || (blob.type !== "image/png" && blob.type !== "image/jpeg" && blob.type !== "video/mp4")) throw new Error("only PNG, JPEG or MP4 blobs are shown");
      if (typeof handle !== "string" || handle === "") throw new Error("an image needs a non-empty handle");
      if (imageUrls[key]) URL.revokeObjectURL(imageUrls[key]);
      imageUrls[key] = URL.createObjectURL(blob);
      var imgs = document.querySelectorAll("img[data-image-from],video[data-image-from]");
      for (var i = 0; i < imgs.length; i++) {{
        if (imgs[i].getAttribute("data-image-from") === key) {{
          var video = imgs[i].tagName.toLowerCase() === "video";
          imgs[i].hidden = video !== (blob.type === "video/mp4");
          if (!imgs[i].hidden) imgs[i].src = imageUrls[key];
          else {{ if (video) imgs[i].pause(); imgs[i].removeAttribute("src"); if (video) imgs[i].load(); }}
        }}
      }}
      this.set(key, handle);
    }},
    clearImage: function (key) {{
      if (!HOST[key]) throw new Error(key + " is not host state");
      if (imageUrls[key]) URL.revokeObjectURL(imageUrls[key]);
      delete imageUrls[key];
      var imgs = document.querySelectorAll("img[data-image-from],video[data-image-from]");
      for (var i = 0; i < imgs.length; i++) {{
        if (imgs[i].getAttribute("data-image-from") === key) {{ if (imgs[i].pause) imgs[i].pause(); imgs[i].removeAttribute("src"); if (imgs[i].load) imgs[i].load(); imgs[i].hidden = true; }}
      }}
      this.set(key, "");
    }},
    errors: function () {{ return errors.slice(); }}
  }};

  var all = document.querySelectorAll("[data-focus]");
  for (var i = 0; i < all.length; i++) {{
    wire(all[i], parseInt(all[i].getAttribute("data-focus"), 10));
  }}
  appearEls = document.querySelectorAll(".w[data-appear]");
  for (i = 0; i < appearEls.length; i++) {{
    var ai = parseInt(appearEls[i].getAttribute("data-appear"), 10);
    wasPresent[ai] = presentNode(ai);
  }}
  sig = signature();
  render();
  focus(CONFIGS[sig] ? CONFIGS[sig].initial : null);
}})();
"#,
        contract = crate::CONTRACT,
        live = crate::live::CONTRACT,
        tables = tables_js(o),
        splash = crate::splash::runtime_js()
    )
}

/// `navigation-graph.json`: every actionable element with its rectangle
/// and successors, using authored ids.
pub fn navigation_report(o: &Ontology) -> String {
    let label = |id: u32| {
        let a = &o.actionable[id as usize];
        json_string(a.authored_id.as_deref().unwrap_or(&a.name))
    };
    let presses = crate::remote::press_counts(&o.graph, o.initial);
    let count = |m: &std::collections::BTreeMap<u32, u32>, id: u32| {
        m.get(&id).map_or("null".to_string(), u32::to_string)
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
                "    {{\"focus\": {}, \"id\": {}, \"presses\": {}, \"presses_home\": {}, \"rect_mp\": [{}, {}, {}, {}], {}}}",
                row.id,
                label(row.id),
                count(&presses.from_initial, row.id),
                count(&presses.to_initial, row.id),
                r.x,
                r.y,
                r.width,
                r.height,
                succ.join(", ")
            )
        })
        .collect();
    format!(
        "{{\n  \"contract\": \"{}\",\n  \"remote_contract\": \"{}\",\n  \"ontology_limit_pt\": [{}, {}],\n  \"initial\": {},\n  \"unreachable\": [],\n  \"traps\": [],\n  \"max_presses\": {},\n  \"max_presses_home\": {},\n  \"elements\": [\n{}\n  ]\n}}\n",
        crate::focus::CONTRACT,
        crate::remote::CONTRACT,
        ONTOLOGY_WIDTH_PT,
        ONTOLOGY_HEIGHT_PT,
        o.initial.map_or("null".into(), label),
        presses.max_from_initial(),
        presses.max_to_initial(),
        rows.join(",\n")
    )
}

/// Arachnid's own 80×80 launcher icon, derived from
/// `assets/arachnid-logo.png` by `scripts/arachnid-icons.py`. Only
/// Arachnid's own apps (`com.goya.arachnid.*`: fixtures and probes) use it.
/// Apps built *with* Arachnid get [`default_icon`] or their own `--icon`;
/// the compiler does not brand other products.
pub const ARACHNID_ICON: &[u8] = include_bytes!("../assets/icon-80.png");

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

    #[test]
    fn arachnid_icon_meets_lgs_launcher_size() {
        assert_eq!(crate::archive::png_dimensions(ARACHNID_ICON), Some((80, 80)));
    }

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
