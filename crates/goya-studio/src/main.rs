use goya_lang::{SceneCell, Vec3};
use goya_sdk::{SceneProject, Viewport};
use std::fs::File;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::Duration;

const TEMPLATE: &str = "@ontology my_app {\n    @cell root {\n        position: (0, 0, 0)\n        extent: (800, 600, 0)\n        fill: #20202a\n\n        @button hello {\n            position: (40, 40, 0)\n            extent: (160, 48, 0)\n            text: \"Hello, Goya\"\n            fill: #3267c8\n        }\n    }\n}\n";

fn usage() {
    println!(
        "goya-studio {}\n\nUsage:\n  goya-studio new <file.gscene>\n  goya-studio check <file.gscene>\n  goya-studio inspect <file.gscene>\n  goya-studio edit <file.gscene> [editor]\n  goya-studio preview <file.gscene> [--live] [scene-binary]\n  goya-studio serve <file.gscene> [port]\n",
        env!("CARGO_PKG_VERSION")
    );
}

const MAX_REQUEST_BYTES: usize = 8 * 1024;
const MAX_SCENE_BYTES: u64 = 128 * 1024;
const MAX_PREVIEW_NODES: usize = 512;

#[derive(Debug, PartialEq, Eq)]
struct HttpRequest {
    method: String,
    target: String,
    host: String,
    origin: Option<String>,
    content_type: Option<String>,
    body: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq)]
struct RequestHead {
    method: String,
    target: String,
    host: String,
    origin: Option<String>,
    content_type: Option<String>,
    content_length: usize,
}

#[derive(Debug, PartialEq, Eq)]
struct RequestError {
    status: &'static str,
    message: &'static str,
}

fn request_error(status: &'static str, message: &'static str) -> RequestError {
    RequestError { status, message }
}

fn parse_request_head(header: &[u8]) -> Result<RequestHead, RequestError> {
    if header.len() > MAX_REQUEST_BYTES {
        return Err(request_error(
            "413 Payload Too Large",
            "Request headers too large",
        ));
    }
    let header = std::str::from_utf8(header)
        .map_err(|_| request_error("400 Bad Request", "Invalid request headers"))?;
    let mut lines = header.split("\r\n");
    let mut request = lines
        .next()
        .ok_or(request_error("400 Bad Request", "Missing request line"))?
        .split_ascii_whitespace();
    let method = request
        .next()
        .ok_or(request_error("400 Bad Request", "Missing method"))?;
    let target = request
        .next()
        .ok_or(request_error("400 Bad Request", "Missing target"))?;
    let version = request
        .next()
        .ok_or(request_error("400 Bad Request", "Missing HTTP version"))?;
    if request.next().is_some() || !matches!(version, "HTTP/1.0" | "HTTP/1.1") {
        return Err(request_error("400 Bad Request", "Invalid request line"));
    }
    if lines.clone().count() > 16 {
        return Err(request_error("400 Bad Request", "Too many headers"));
    }
    let (mut host, mut origin, mut content_type, mut content_length) = (None, None, None, None);
    for line in lines {
        let (name, value) = line
            .split_once(':')
            .ok_or(request_error("400 Bad Request", "Invalid header"))?;
        let value = value.trim();
        let slot = match name.to_ascii_lowercase().as_str() {
            "host" => &mut host,
            "origin" => &mut origin,
            "content-type" => &mut content_type,
            "content-length" => {
                if content_length.is_some() {
                    return Err(request_error("400 Bad Request", "Duplicate Content-Length"));
                }
                let length = value
                    .parse::<usize>()
                    .map_err(|_| request_error("400 Bad Request", "Invalid Content-Length"))?;
                content_length = Some(length);
                continue;
            }
            "transfer-encoding" => {
                return Err(request_error(
                    "400 Bad Request",
                    "Transfer-Encoding is not supported",
                ))
            }
            _ => continue,
        };
        if slot.is_some() {
            return Err(request_error(
                "400 Bad Request",
                "Duplicate security header",
            ));
        }
        *slot = Some(value.to_owned());
    }
    let host = host.ok_or(request_error("400 Bad Request", "Host header is required"))?;
    let content_length = content_length.unwrap_or(0);
    if content_length > MAX_SCENE_BYTES as usize {
        return Err(request_error(
            "413 Payload Too Large",
            "Scene source exceeds the 128 KiB limit",
        ));
    }
    Ok(RequestHead {
        method: method.to_owned(),
        target: target.to_owned(),
        host,
        origin,
        content_type,
        content_length,
    })
}

fn parse_request(bytes: &[u8]) -> Result<HttpRequest, RequestError> {
    if bytes.len() > MAX_REQUEST_BYTES + MAX_SCENE_BYTES as usize {
        return Err(request_error("413 Payload Too Large", "Request too large"));
    }
    let end = bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| {
            if bytes.len() > MAX_REQUEST_BYTES {
                request_error("413 Payload Too Large", "Request headers too large")
            } else {
                request_error("400 Bad Request", "Incomplete request headers")
            }
        })?;
    let RequestHead {
        method,
        target,
        host,
        origin,
        content_type,
        content_length,
    } = parse_request_head(&bytes[..end])?;
    let body = &bytes[end + 4..];
    if body.len() != content_length {
        return Err(request_error(
            "400 Bad Request",
            "Content-Length does not match request body",
        ));
    }
    Ok(HttpRequest {
        method,
        target,
        host,
        origin,
        content_type,
        body: body.to_vec(),
    })
}

fn escape_html(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn render_html(project: &SceneProject) -> String {
    let viewport = project.viewport();
    let mut html = format!("<!doctype html><html><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{}</title><style>body{{margin:0;background:#111;color:#eee;font:14px system-ui,sans-serif}}header{{padding:12px 18px}}main{{padding:18px;overflow:auto}}.stage{{position:relative;margin:auto;background:#20202a;box-shadow:0 4px 24px #0008}}.node{{position:absolute;box-sizing:border-box;overflow:hidden;white-space:pre-wrap;padding:4px;border:1px solid #ffffff30;border-radius:3px}}.button{{display:flex;align-items:center;justify-content:center;font:inherit}}</style></head><body><header>Goya Studio preview · {}</header><main><div class=\"stage\" style=\"width:{}px;height:{}px\">", escape_html(&project.scene().name), escape_html(&project.scene().name), viewport.width_points, viewport.height_points);
    for (index, node) in project.frame().nodes.into_iter().enumerate() {
        let color = node
            .fill
            .map(|color| format!("#{:06x}", color & 0x00ff_ffff))
            .unwrap_or_else(|| "#353545".into());
        let label = node.text.as_deref().unwrap_or(&node.name);
        let role = if node.is_button { "button" } else { "group" };
        html.push_str(&format!("<div class=\"node{}\" data-node-index=\"{}\" role=\"{}\" aria-label=\"{}\" style=\"left:{}px;top:{}px;width:{}px;height:{}px;background:{};{}\">{}</div>",
            if node.is_button { " button" } else { "" }, index, role, escape_html(label), node.rect.x as f64 / 1000.0,
            node.rect.y as f64 / 1000.0, node.rect.width as f64 / 1000.0, node.rect.height as f64 / 1000.0,
            color, node.text_size.map(|size| format!("font-size:{}px;", size)).unwrap_or_default(), escape_html(label)));
    }
    html.push_str("</div></main><script>const parentWindow=window.parent,nodes=[...document.querySelectorAll('[data-node-index]')];let selected=-1,drag=null;function markSelected(i){selected=i;for(const el of nodes)el.style.outline=Number(el.dataset.nodeIndex)===i?'2px solid #ffdc65':'';}function select(i){markSelected(i);parentWindow.postMessage({type:'goya-select',index:i},'*');}window.addEventListener('message',e=>{if(e.source!==parentWindow||!e.data||e.data.type!=='goya-selected'||!Number.isInteger(e.data.index))return;markSelected(e.data.index);});for(const el of nodes){const index=Number(el.dataset.nodeIndex);el.style.cursor='grab';el.style.touchAction='none';el.addEventListener('pointerdown',e=>{if(e.button!==0)return;e.preventDefault();select(index);drag={index,x:e.clientX,y:e.clientY,el};el.setPointerCapture(e.pointerId);});el.addEventListener('pointermove',e=>{if(!drag||drag.index!==index)return;drag.el.style.transform='translate('+(e.clientX-drag.x)+'px,'+(e.clientY-drag.y)+'px)';});el.addEventListener('pointerup',e=>{if(!drag||drag.index!==index)return;const d={type:'goya-drag',index,dx:e.clientX-drag.x,dy:e.clientY-drag.y};drag.el.style.transform='';drag=null;if(Math.abs(d.dx)+Math.abs(d.dy)>1)parentWindow.postMessage(d,'*');});el.addEventListener('pointercancel',()=>{if(drag&&drag.index===index){drag.el.style.transform='';drag=null;}});}</script></body></html>");
    html
}

fn render_error_html(message: &str) -> String {
    let bounded = message.chars().take(1024).collect::<String>();
    format!("<!doctype html><meta charset=\"utf-8\"><title>Scene validation failed</title><style>body{{background:#17171d;color:#eee;font:14px system-ui;padding:20px}}pre{{white-space:pre-wrap;color:#ffb8b8}}</style><h2>Scene validation failed</h2><pre>{}</pre>", escape_html(&bounded))
}

fn flatten_cells<'a>(
    cell: &'a SceneCell,
    parent: Option<usize>,
    out: &mut Vec<(&'a SceneCell, Option<usize>)>,
) {
    let index = out.len();
    out.push((cell, parent));
    for child in &cell.children {
        flatten_cells(child, Some(index), out);
    }
}

fn finite_or(value: Option<Vec3>, fallback: Vec3) -> Vec3 {
    let value = value.unwrap_or(fallback);
    Vec3 {
        x: if value.x.is_finite() { value.x } else { 0.0 },
        y: if value.y.is_finite() { value.y } else { 0.0 },
        z: if value.z.is_finite() { value.z } else { 0.0 },
    }
}

fn render_editor(project: &SceneProject, preview: &str) -> String {
    let source = project.source();
    let mut cells = Vec::new();
    flatten_cells(&project.scene().root, None, &mut cells);
    let frame = project.frame();
    let mut html = String::from("<!doctype html><html><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Goya Studio</title><style>body{margin:0;background:#17171d;color:#eee;font:14px system-ui,sans-serif}header{padding:10px 16px;border-bottom:1px solid #444}main{display:grid;grid-template-columns:minmax(0,1fr) minmax(0,1fr);gap:12px;padding:12px;height:calc(100vh - 65px);height:calc(100dvh - 65px);box-sizing:border-box}section{display:flex;flex-direction:column;min-height:0}.tools{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:8px;padding:8px 0}textarea{flex:1;resize:none;background:#101014;color:#e8e8f0;border:1px solid #777;padding:12px;font:13px/1.5 ui-monospace,monospace}input,select{min-height:36px;background:#101014;color:#eee;border:1px solid #777;padding:5px}button{margin-top:8px;min-height:44px;padding:9px 12px;background:#3267c8;color:white;border:0;border-radius:4px;cursor:pointer}iframe{width:100%;height:100%;border:1px solid #777;background:#111}button:focus-visible,textarea:focus-visible,iframe:focus-visible,input:focus-visible,select:focus-visible{outline:3px solid #9fc2ff;outline-offset:2px}output{min-height:1.4em;padding:5px}@media(max-width:760px){main{grid-template-columns:minmax(0,1fr);grid-template-rows:minmax(18rem,1fr) minmax(18rem,1fr);height:auto;min-height:calc(100vh - 65px);min-height:calc(100dvh - 65px)}}@media(forced-colors:active){button:focus-visible,textarea:focus-visible,iframe:focus-visible,input:focus-visible,select:focus-visible{outline:3px solid Highlight;outline-offset:2px}}</style></head><body><header><strong>Goya Studio</strong> · local visual editor and preview</header><main aria-label=\"Scene editor\"><section aria-labelledby=\"source-label\"><label id=\"source-label\" for=\"source\">Scene source</label><div class=\"tools\"><label>Node<select id=\"node\">");
    for (index, (cell, parent_index)) in cells.iter().enumerate() {
        let draw = frame.nodes.get(index);
        let pos_default = Vec3::default();
        let extent_default = draw
            .map(|n| Vec3 {
                x: n.rect.width as f64 / 1000.0,
                y: n.rect.height as f64 / 1000.0,
                z: 0.0,
            })
            .unwrap_or_default();
        let pos_key = if cell.position_p.is_some() {
            "position_p"
        } else {
            "position"
        };
        let extent_key = if cell.extent_p.is_some() {
            "extent_p"
        } else {
            "extent"
        };
        let pos = finite_or(cell.position_p.or(cell.position), pos_default);
        let extent = finite_or(cell.extent_p.or(cell.extent), extent_default);
        let kind = if cell.kind == goya_lang::CellKind::Button {
            "button"
        } else {
            "cell"
        };
        let parent_extent = parent_index
            .and_then(|parent| frame.nodes.get(parent))
            .map(|parent| {
                (
                    parent.rect.width as f64 / 1000.0,
                    parent.rect.height as f64 / 1000.0,
                )
            })
            .unwrap_or((
                project.viewport().width_points,
                project.viewport().height_points,
            ));
        html.push_str(&format!("<option value=\"{}\" data-name=\"{}\" data-kind=\"{}\" data-pos-key=\"{}\" data-extent-key=\"{}\" data-parent-w=\"{}\" data-parent-h=\"{}\" data-x=\"{}\" data-y=\"{}\" data-w=\"{}\" data-h=\"{}\">{}</option>", index, escape_html(&cell.name), kind, pos_key, extent_key, parent_extent.0, parent_extent.1, pos.x, pos.y, extent.x, extent.y, escape_html(&cell.name)));
    }
    html.push_str("</select></label><label>Position mode<select id=\"pos-mode\"><option value=\"position\">Points</option><option value=\"position_p\">Parent fraction</option></select></label><label>Extent mode<select id=\"extent-mode\"><option value=\"extent\">Points</option><option value=\"extent_p\">Parent fraction</option></select></label><label>X<input id=\"x\" type=\"number\" step=\"any\"></label><label>Y<input id=\"y\" type=\"number\" step=\"any\"></label><label>Width<input id=\"w\" type=\"number\" step=\"any\"></label><label>Height<input id=\"h\" type=\"number\" step=\"any\"></label></div><textarea id=\"source\" spellcheck=\"false\">");
    html.push_str(&escape_html(source));
    html.push_str("</textarea><div><button id=\"apply\" type=\"button\">Apply geometry</button> <button id=\"update\">Update preview</button> <button id=\"download\" type=\"button\">Download source</button><output id=\"status\" aria-live=\"polite\"></output></div></section><section aria-labelledby=\"preview-label\"><label id=\"preview-label\">Preview</label><iframe id=\"preview\" title=\"Preview canvas; drag selected nodes to reposition\" sandbox=\"allow-scripts\"></iframe></section></main><script>const source=document.querySelector('#source'),frame=document.querySelector('#preview'),status=document.querySelector('#status'),node=document.querySelector('#node');frame.srcdoc='");
    html.push_str(&escape_js_string(preview));
    html.push_str("';const fields=['x','y','w','h'].map(id=>document.querySelector('#'+id));function loadNode(){const o=node.selectedOptions[0];document.querySelector('#pos-mode').value=o.dataset.posKey;document.querySelector('#extent-mode').value=o.dataset.extentKey;fields.forEach((f,i)=>f.value=[o.dataset.x,o.dataset.y,o.dataset.w,o.dataset.h][i]);}node.onchange=loadNode;loadNode();function blocks(text){const out=[];function closeAt(open){let d=1,q=false,esc=false,line=false;for(let i=open+1;i<text.length;i++){const c=text[i],n=text[i+1];if(line){if(c==='\\n')line=false;continue;}if(q){if(esc)esc=false;else if(c==='\\\\')esc=true;else if(c==='\\\"')q=false;continue;}if(c==='\\\"')q=true;else if(c==='/'&&n==='/'){line=true;i++;}else if(c==='{')d++;else if(c==='}'&&--d===0)return i;}return -1;}function scan(from,to){let q=false,esc=false,line=false;for(let i=from;i<to;i++){const c=text[i],n=text[i+1];if(line){if(c==='\\n')line=false;continue;}if(q){if(esc)esc=false;else if(c==='\\\\')esc=true;else if(c==='\\\"')q=false;continue;}if(c==='\\\"'){q=true;continue;}if(c==='/'&&n==='/'){line=true;i++;continue;}if(c==='@'){const m=/^@(cell|button)\\s+([A-Za-z_][A-Za-z0-9_-]*)\\s*\\{/.exec(text.slice(i));if(m){const open=i+m[0].lastIndexOf('{'),end=closeAt(open);if(end<0)throw Error('Unclosed scene node');out.push({open,end});scan(open+1,end);i=end;}}}}scan(0,text.length);return out;}function editBlock(text,b,pk,ek,vals){let body=text.slice(b.open+1,b.end),lines=body.split('\\n'),depth=1,pDone=false,eDone=false;for(let i=0;i<lines.length;i++){const line=lines[i];if(depth===1){const m=/^(\\s*)(position(?:_p)?|extent(?:_p)?)\\s*:\\s*.*$/.exec(line);if(m){const k=m[2],isP=k.startsWith('position');if(isP){if(!pDone){lines[i]=m[1]+pk+': ('+vals[0]+', '+vals[1]+', 0)';pDone=true;}else lines[i]='';}else if(!eDone){lines[i]=m[1]+ek+': ('+vals[2]+', '+vals[3]+', 0)';eDone=true;}else lines[i]='';}}let q=false,esc=false,comment=false;for(let j=0;j<line.length;j++){const c=line[j],n=line[j+1];if(comment)break;if(q){if(esc)esc=false;else if(c==='\\\\')esc=true;else if(c==='\\\"')q=false;}else if(c==='\\\"')q=true;else if(c==='/'&&n==='/'){comment=true;break;}else if(c==='{')depth++;else if(c==='}')depth--;}}const indent=(lines.find(l=>l.trim())||'').match(/^\\s*/)[0]+'    ';if(!pDone)lines.splice(Math.max(0,lines.length-1),0,indent+pk+': ('+vals[0]+', '+vals[1]+', 0)');if(!eDone)lines.splice(Math.max(0,lines.length-1),0,indent+ek+': ('+vals[2]+', '+vals[3]+', 0)');return text.slice(0,b.open+1)+lines.join('\\n')+text.slice(b.end);}document.querySelector('#apply').onclick=()=>{const o=node.selectedOptions[0],v=fields.map(f=>f.valueAsNumber),pm=document.querySelector('#pos-mode').value,em=document.querySelector('#extent-mode').value;if(v.some(x=>!Number.isFinite(x))){status.textContent='Enter four finite geometry values';return;}if((pm==='position_p'&&(v[0]<0||v[0]>1||v[1]<0||v[1]>1))||(em==='extent_p'&&(v[2]<0||v[2]>1||v[3]<0||v[3]>1))||v[2]<0||v[3]<0||v.some(x=>Math.abs(x)>32768)){status.textContent='Geometry is outside the supported range';return;}try{if(new TextEncoder().encode(source.value).length>131072)throw Error('Scene exceeds the 128 KiB limit');const bs=blocks(source.value),b=bs[Number(o.value)];if(!b)throw Error('Could not match selected scene node');source.value=editBlock(source.value,b,pm,em,v);status.textContent='Geometry updated in memory. Download source to keep it.';}catch(e){status.textContent=e.message==='Scene exceeds the 128 KiB limit'?e.message:'Could not safely update this node';}};document.querySelector('#update').onclick=async e=>{e.preventDefault();status.textContent='Validating…';try{const r=await fetch('/preview',{method:'POST',headers:{'Content-Type':'text/plain;charset=utf-8'},body:source.value});frame.srcdoc=await r.text();status.textContent=r.ok?'Preview updated':'Validation failed';}catch(e){status.textContent='Preview request failed';}};document.querySelector('#download').onclick=()=>{const a=document.createElement('a');a.href=URL.createObjectURL(new Blob([source.value],{type:'text/plain'}));a.download='scene.gscene';a.click();setTimeout(()=>URL.revokeObjectURL(a.href),1000);};</script></body></html>");
    html = html
        .replace(
            "node.onchange=loadNode;loadNode();",
            "node.onchange=()=>{loadNode();frame.contentWindow.postMessage({type:'goya-selected',index:Number(node.value)},'*')};loadNode();frame.onload=()=>frame.contentWindow.postMessage({type:'goya-selected',index:Number(node.value)},'*');window.addEventListener('message',e=>{if(e.source!==frame.contentWindow||!e.data||!Number.isInteger(e.data.index))return;const i=e.data.index;if(e.data.type==='goya-select'){if(!node.options[i])return;node.value=String(i);loadNode();return;}if(e.data.type!=='goya-drag'||i!==Number(node.value)||source.value!==inspectorSource)return;let dx=e.data.dx,dy=e.data.dy;if(!Number.isFinite(dx)||!Number.isFinite(dy)||Math.abs(dx)>32768||Math.abs(dy)>32768)return;const o=node.selectedOptions[0],pm=document.querySelector('#pos-mode').value;if(pm==='position_p'){const pw=Number(o.dataset.parentW),ph=Number(o.dataset.parentH);if(!(pw>0&&ph>0))return;dx/=pw;dy/=ph;}fields[0].value=String(fields[0].valueAsNumber+dx);fields[1].value=String(fields[1].valueAsNumber+dy);document.querySelector('#apply').click();});",
        )
        .replace(
            "function editBlock(text,b,pk,ek,vals){",
            "let dragPositionOnly=false;function editBlock(text,b,pk,ek,vals,positionOnly=false){",
        )
        .replace(
            "}else if(!eDone){",
            "}else if(positionOnly){}else if(!eDone){",
        )
        .replace("if(!eDone)lines.splice(", "if(!eDone&&!positionOnly)lines.splice(")
        .replace("source.value=editBlock(source.value,b,pm,em,v);", "source.value=editBlock(source.value,b,pm,em,v,dragPositionOnly);")
        .replace(
            "document.querySelector('#apply').click();});",
            "dragPositionOnly=true;document.querySelector('#apply').click();dragPositionOnly=false;});",
        )
        .replace("out.push({open,end});", "out.push({open,end,name:m[2],kind:m[1]});")
        .replace(
            "/^@(cell|button)\\s+([A-Za-z_][A-Za-z0-9_-]*)\\s*\\{/",
            "/^@(cell|button)\\s+([\\p{L}_][\\p{L}\\p{N}_-]*)\\s*\\{/u",
        )
        .replace("if(!b)throw Error('Could not match selected scene node');", "if(!b||b.name!==o.dataset.name||b.kind!==o.dataset.kind)throw Error('Could not match selected scene node');")
        .replace("if(new TextEncoder().encode(source.value).length>131072)throw Error('Scene exceeds the 128 KiB limit');", "if(source.value!==inspectorSource)throw Error('Scene structure changed; reload the editor before using the inspector');if(new TextEncoder().encode(source.value).length>131072)throw Error('Scene exceeds the 128 KiB limit');")
        .replace("source.value=editBlock(source.value,b,pm,em,v);status.textContent=", "source.value=editBlock(source.value,b,pm,em,v);inspectorSource=source.value;status.textContent=")
        .replace("';const fields=", "';let inspectorSource=source.value;const fields=");
    html
}

fn escape_js_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\'' => out.push_str("\\'"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            '<' => out.push_str("\\x3c"),
            '>' => out.push_str("\\x3e"),
            '&' => out.push_str("\\x26"),
            _ => out.push(ch),
        }
    }
    out
}

fn route_request(
    request: &[u8],
    expected_host: &str,
    source: &str,
    initial_preview: &str,
) -> (&'static str, &'static str, String) {
    let req = match parse_request(request) {
        Ok(req) => req,
        Err(error) => {
            return (
                error.status,
                "text/html; charset=utf-8",
                render_error_html(error.message),
            )
        }
    };
    if req.host != expected_host {
        return (
            "403 Forbidden",
            "text/plain; charset=utf-8",
            "Host rejected".into(),
        );
    }
    if req.method == "GET" && req.target == "/" && req.body.is_empty() {
        return (
            "200 OK",
            "text/html; charset=utf-8",
            match SceneProject::from_source(source, Viewport::default()) {
                Ok(project) => render_editor(&project, initial_preview),
                Err(error) => render_error_html(&error.to_string()),
            },
        );
    }
    if req.method == "POST" && req.target == "/preview" {
        let expected_origin = format!("http://{expected_host}");
        if req.origin.as_deref() != Some(expected_origin.as_str()) {
            return (
                "403 Forbidden",
                "text/plain; charset=utf-8",
                "Origin rejected".into(),
            );
        }
        if req.content_type.as_deref() != Some("text/plain;charset=utf-8") {
            return (
                "415 Unsupported Media Type",
                "text/plain; charset=utf-8",
                "Use text/plain;charset=utf-8".into(),
            );
        }
        let source = match String::from_utf8(req.body) {
            Ok(source) => source,
            Err(_) => {
                return (
                    "400 Bad Request",
                    "text/plain; charset=utf-8",
                    "Scene source must be UTF-8".into(),
                )
            }
        };
        return match SceneProject::from_source(source, Viewport::default()) {
            Ok(project) if project.frame().nodes.len() <= MAX_PREVIEW_NODES => {
                ("200 OK", "text/html; charset=utf-8", render_html(&project))
            }
            Ok(_) => (
                "422 Unprocessable Content",
                "text/html; charset=utf-8",
                render_error_html("Scene exceeds the 512-node preview limit"),
            ),
            Err(error) => (
                "422 Unprocessable Content",
                "text/html; charset=utf-8",
                render_error_html(&error.to_string()),
            ),
        };
    }
    if req.method != "GET" && req.method != "POST" {
        return (
            "405 Method Not Allowed",
            "text/plain; charset=utf-8",
            "Method Not Allowed".into(),
        );
    }
    (
        "404 Not Found",
        "text/plain; charset=utf-8",
        "Not Found".into(),
    )
}

fn read_http_request(stream: &mut TcpStream) -> Result<Vec<u8>, RequestError> {
    let mut request = Vec::new();
    let mut chunk = [0u8; 1024];
    let (head_end, content_length);
    loop {
        let count = stream
            .read(&mut chunk)
            .map_err(|_| request_error("400 Bad Request", "Could not read request"))?;
        if count == 0 {
            return Err(request_error("400 Bad Request", "Incomplete request"));
        }
        request.extend_from_slice(&chunk[..count]);
        if request.len() > MAX_REQUEST_BYTES + MAX_SCENE_BYTES as usize {
            return Err(request_error("413 Payload Too Large", "Request too large"));
        }
        if let Some(end) = request.windows(4).position(|window| window == b"\r\n\r\n") {
            let length = parse_request_head(&request[..end])?.content_length;
            if length > MAX_SCENE_BYTES as usize {
                return Err(request_error(
                    "413 Payload Too Large",
                    "Scene source exceeds the 128 KiB limit",
                ));
            }
            head_end = end + 4;
            content_length = length;
            break;
        }
        if request.len() > MAX_REQUEST_BYTES {
            return Err(request_error(
                "413 Payload Too Large",
                "Request headers too large",
            ));
        }
    }
    let expected = head_end + content_length;
    while request.len() < expected {
        let count = stream
            .read(&mut chunk)
            .map_err(|_| request_error("400 Bad Request", "Could not read request body"))?;
        if count == 0 {
            return Err(request_error("400 Bad Request", "Incomplete request body"));
        }
        request.extend_from_slice(&chunk[..count]);
        if request.len() > expected {
            return Err(request_error(
                "400 Bad Request",
                "Request body exceeds Content-Length",
            ));
        }
    }
    Ok(request)
}

fn respond(
    stream: &mut TcpStream,
    request: &[u8],
    expected_host: &str,
    source: &str,
    initial_preview: &str,
) -> std::io::Result<()> {
    let (status, content_type, body) =
        route_request(request, expected_host, source, initial_preview);
    write!(stream, "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\nX-Content-Type-Options: nosniff\r\nContent-Security-Policy: default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'; connect-src 'self'; frame-src 'self'\r\n\r\n{}", body.len(), body)
}

fn serve(path: &Path, port: u16) -> Result<(), String> {
    let mut source = Vec::with_capacity(MAX_SCENE_BYTES as usize + 1);
    File::open(path)
        .map_err(|error| format!("cannot open scene: {error}"))?
        .take(MAX_SCENE_BYTES + 1)
        .read_to_end(&mut source)
        .map_err(|error| format!("cannot read scene: {error}"))?;
    if source.len() as u64 > MAX_SCENE_BYTES {
        return Err(format!(
            "scene exceeds the {} KiB preview limit",
            MAX_SCENE_BYTES / 1024
        ));
    }
    let source =
        String::from_utf8(source).map_err(|error| format!("scene is not UTF-8: {error}"))?;
    let scene = SceneProject::from_source(source.clone(), Viewport::default())
        .map_err(|error| error.to_string())?;
    if scene.frame().nodes.len() > MAX_PREVIEW_NODES {
        return Err(format!(
            "scene exceeds the {MAX_PREVIEW_NODES}-node preview limit"
        ));
    }
    let listener = TcpListener::bind(("127.0.0.1", port))
        .map_err(|error| format!("cannot bind local preview: {error}"))?;
    println!(
        "Studio preview: http://{} (Ctrl-C to stop)",
        listener.local_addr().map_err(|error| error.to_string())?
    );
    for incoming in listener.incoming() {
        let Ok(mut stream) = incoming else { continue };
        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
        let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
        match read_http_request(&mut stream) {
            Ok(request) => {
                let _ = respond(
                    &mut stream,
                    &request,
                    &listener.local_addr().unwrap().to_string(),
                    &source,
                    &render_html(&scene),
                );
            }
            Err(error) => {
                let body = render_error_html(error.message);
                let _ = write!(stream, "HTTP/1.1 {}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", error.status, body.len(), body);
            }
        }
    }
    Ok(())
}

fn project(path: &Path) -> Result<SceneProject, String> {
    SceneProject::from_path(path, Viewport::default()).map_err(|error| error.to_string())
}

fn check(path: &Path) -> Result<(), String> {
    let project = project(path)?;
    println!(
        "valid: ontology={} scene_nodes={} ir_nodes={} viewport={}x{}pt",
        project.scene().name,
        project.frame().nodes.len(),
        project.module().nodes.len(),
        project.viewport().width_points,
        project.viewport().height_points
    );
    Ok(())
}

fn inspect(path: &Path) -> Result<(), String> {
    let project = project(path)?;
    println!("Scene: {}", project.scene().name);
    let frame = project.frame();
    for node in &frame.nodes {
        println!(
            "{} {} x={} y={} w={} h={} text={:?}",
            if node.is_button { "button" } else { "cell" },
            node.name,
            node.rect.x,
            node.rect.y,
            node.rect.width,
            node.rect.height,
            node.text
        );
    }
    Ok(())
}

fn edit(path: &Path, editor: Option<&str>) -> Result<(), String> {
    if !path.exists() {
        std::fs::write(path, TEMPLATE).map_err(|error| format!("cannot create scene: {error}"))?;
    }
    let editor = editor
        .map(str::to_owned)
        .or_else(|| std::env::var("VISUAL").ok())
        .or_else(|| std::env::var("EDITOR").ok())
        .ok_or_else(|| "set VISUAL/EDITOR or pass the editor executable".to_string())?;
    let status = Command::new(&editor)
        .arg(path)
        .status()
        .map_err(|error| format!("cannot launch editor '{editor}': {error}"))?;
    if !status.success() {
        return Err(format!("editor exited with {status}"));
    }
    check(path)
}

fn preview(path: &Path, live: bool, scene_binary: Option<&str>) -> Result<(), String> {
    // Validate before preview launch so malformed edits never replace the
    // user's last known-good running scene.
    check(path)?;
    if live {
        let binary = scene_binary.unwrap_or("goya-scene");
        let status = Command::new(binary)
            .arg("--file")
            .arg(path)
            .arg("--live")
            .status()
            .map_err(|error| format!("cannot launch scene preview '{binary}': {error}"))?;
        if !status.success() {
            return Err(format!("scene preview exited with {status}"));
        }
    } else {
        inspect(path)?;
    }
    Ok(())
}

fn run(args: &[String]) -> Result<(), String> {
    let Some(command) = args.first().map(String::as_str) else {
        usage();
        return Ok(());
    };
    match command {
        "--help" | "-h" | "help" => usage(),
        "new" => {
            let path = args.get(1).ok_or_else(|| "new needs a path".to_string())?;
            let path = PathBuf::from(path);
            if path.exists() {
                return Err(format!("refusing to overwrite {}", path.display()));
            }
            std::fs::write(&path, TEMPLATE).map_err(|error| error.to_string())?;
            check(&path)?;
            println!("created {}", path.display());
        }
        "check" => {
            let path = args
                .get(1)
                .ok_or_else(|| "check needs a path".to_string())?;
            check(Path::new(path))?;
        }
        "inspect" => {
            let path = args
                .get(1)
                .ok_or_else(|| "inspect needs a path".to_string())?;
            inspect(Path::new(path))?;
        }
        "edit" => {
            let path = args.get(1).ok_or_else(|| "edit needs a path".to_string())?;
            edit(Path::new(path), args.get(2).map(String::as_str))?;
        }
        "preview" => {
            let path = args
                .get(1)
                .ok_or_else(|| "preview needs a path".to_string())?;
            let live = args.iter().any(|arg| arg == "--live");
            let binary = args
                .iter()
                .skip(2)
                .find(|arg| arg.as_str() != "--live")
                .map(String::as_str);
            preview(Path::new(path), live, binary)?;
        }
        "serve" => {
            let path = args
                .get(1)
                .ok_or_else(|| "serve needs a path".to_string())?;
            let port = args
                .get(2)
                .map(|value| {
                    value
                        .parse::<u16>()
                        .map_err(|_| "port must be an integer from 0 to 65535".to_string())
                })
                .transpose()?
                .unwrap_or(8765);
            serve(Path::new(path), port)?;
        }
        other => return Err(format!("unknown command: {other}")),
    }
    Ok(())
}

fn main() -> ExitCode {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("goya-studio: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOST: &str = "127.0.0.1:8765";
    const SCENE: &str =
        "@ontology demo { @button ok { position: (0, 0, 0) extent: (80, 40, 0) text: \"hello\" } }";

    fn get(path: &str) -> Vec<u8> {
        format!("GET {path} HTTP/1.1\r\nHost: {HOST}\r\n\r\n").into_bytes()
    }

    fn post(path: &str, body: &str) -> Vec<u8> {
        format!("POST {path} HTTP/1.1\r\nHost: {HOST}\r\nOrigin: http://{HOST}\r\nContent-Type: text/plain;charset=utf-8\r\nContent-Length: {}\r\n\r\n{body}", body.len()).into_bytes()
    }

    #[test]
    fn no_args_prints_usage_without_error() {
        assert!(run(&[]).is_ok());
    }

    #[test]
    fn invalid_command_is_rejected() {
        assert!(run(&["publish".into()]).is_err());
    }

    #[test]
    fn request_parser_enforces_declared_and_bounded_bodies() {
        let parsed = parse_request(&get("/")).unwrap();
        assert_eq!(
            parsed,
            HttpRequest {
                method: "GET".into(),
                target: "/".into(),
                host: HOST.into(),
                origin: None,
                content_type: None,
                body: Vec::new()
            }
        );
        assert_eq!(
            parse_request(&post("/preview", "short")).unwrap().body,
            b"short"
        );
        assert!(
            parse_request(&post("/preview", "short")[..post("/preview", "short").len() - 1])
                .is_err()
        );
        let huge_head = format!(
            "POST /preview HTTP/1.1\r\nHost: {HOST}\r\nContent-Length: {}\r\n\r\n",
            MAX_SCENE_BYTES + 1
        );
        assert_eq!(
            parse_request(huge_head.as_bytes()).unwrap_err().status,
            "413 Payload Too Large"
        );
        assert_eq!(
            parse_request(&vec![b'x'; MAX_REQUEST_BYTES + 1])
                .unwrap_err()
                .status,
            "413 Payload Too Large"
        );
    }

    #[test]
    fn preview_routes_and_escapes_scene_content() {
        let project = SceneProject::from_source("@ontology demo { @button ok { position: (0, 0, 0) extent: (80, 40, 0) text: \"<script>\" } }", Viewport::default()).unwrap();
        let html = render_html(&project);
        assert!(html.contains("&lt;script&gt;"));
        assert!(html.contains("<script>")); // fixed canvas interaction code
        assert!(!html.contains("<script>alert(1)</script>"));
        let initial = route_request(&get("/"), HOST, SCENE, &html);
        assert_eq!(initial.0, "200 OK");
        assert!(initial.2.contains("Scene source"));
        assert!(initial.2.contains("textarea"));
        assert!(initial.2.contains("Apply geometry"));
        assert!(initial.2.contains("Parent fraction"));
        assert!(initial.2.contains("data-node-index=\"0\""));
        assert!(initial.2.contains("data-kind=\"button\""));
        assert!(initial
            .2
            .contains("Scene structure changed; reload the editor"));
        assert!(initial.2.contains("data-name=\"ok\""));
        assert!(initial.2.contains("sandbox=\"allow-scripts\""));
        assert!(initial.2.contains("e.source!==frame.contentWindow"));
        assert!(initial.2.contains("dragPositionOnly=true"));
        assert!(initial.2.contains("positionOnly){}else if(!eDone)"));
        let preview = route_request(&post("/preview", SCENE), HOST, SCENE, &html);
        assert!(preview.2.contains("pointerup"));
        assert!(preview.2.contains("goya-drag"));
        assert!(initial.2.contains("Update preview"));
        assert_eq!(
            route_request(&get("/other"), HOST, SCENE, &html).0,
            "404 Not Found"
        );
        assert_eq!(
            route_request(&post("/preview", SCENE), HOST, SCENE, &html).0,
            "200 OK"
        );
        let invalid = route_request(
            &post("/preview", "<script>alert(1)</script>"),
            HOST,
            SCENE,
            &html,
        );
        assert_eq!(invalid.0, "422 Unprocessable Content");
        assert!(invalid.2.contains("Scene validation failed"));
        assert!(!invalid.2.contains("<script>"));
        let mut foreign = post("/preview", SCENE);
        let foreign_text = String::from_utf8(foreign.clone()).unwrap().replace(
            "Origin: http://127.0.0.1:8765",
            "Origin: http://attacker.invalid",
        );
        foreign = foreign_text.into_bytes();
        assert_eq!(
            route_request(&foreign, HOST, SCENE, &html).0,
            "403 Forbidden"
        );
        let wrong_type = String::from_utf8(post("/preview", SCENE))
            .unwrap()
            .replace("text/plain;charset=utf-8", "application/json");
        assert_eq!(
            route_request(wrong_type.as_bytes(), HOST, SCENE, &html).0,
            "415 Unsupported Media Type"
        );
        assert_eq!(
            route_request(&post("/preview", SCENE), "localhost:8765", SCENE, &html).0,
            "403 Forbidden"
        );
        assert_eq!(
            route_request(&post("/", SCENE), HOST, SCENE, &html).0,
            "404 Not Found"
        );
        assert_eq!(
            route_request(b"broken", HOST, SCENE, &html).0,
            "400 Bad Request"
        );
    }
}
