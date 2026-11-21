//! Deterministic experimental static output. No application logic is emitted.
use maja_semantic::StaticWebIr;
use sha2::{Digest, Sha256};
use std::{fs, io, io::Write, path::Path};

fn html_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(result, "{byte:02x}").expect("writing to String cannot fail");
    }
    result
}

/// Write a new output directory only. Existing outputs are never overwritten.
pub fn write_static(ir: &StaticWebIr, output: &Path) -> io::Result<()> {
    let html = format!(concat!(
        "<!doctype html>\n<html><head><meta charset=\"utf-8\">",
        "<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">",
        "<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'self'; img-src data:; base-uri 'none'; form-action 'none'\">",
        "<link rel=\"icon\" href=\"data:,\">",
        "<link rel=\"stylesheet\" href=\"app.css\"><title>Goya application</title></head>",
        "<body><main data-goya-id=\"{}\">{}</main></body></html>\n"
    ), html_escape(ir.semantic_id()), html_escape(ir.text()));
    let css = "html,body{margin:0;width:100%;height:100%}main{box-sizing:border-box;height:100%;white-space:pre;overflow:hidden}\n";
    let origin = ir.origin();
    let manifest = format!(
        concat!(
            "{{\n  \"schema\": \"MAJA-STATIC/1-draft\",\n",
            "  \"compiler\": \"{}\",\n  \"profile\": \"maja.web.local.v1\",\n",
            "  \"qualification\": \"experimental\",\n  \"source_sha256\": \"{}\",\n",
            "  \"text_source_bytes\": [ {}, {} ],\n",
            "  \"artifacts_sha256\": {{ \"index.html\": \"{}\", \"app.css\": \"{}\" }}\n}}\n"
        ),
        env!("CARGO_PKG_VERSION"),
        hex(&ir.source_digest()),
        origin.start,
        origin.end,
        hex(&Sha256::digest(html.as_bytes())),
        hex(&Sha256::digest(css.as_bytes()))
    );

    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(output)?;
    let mut created = Vec::new();
    let result = (|| {
        for (name, bytes) in [
            ("index.html", html.as_bytes()),
            ("app.css", css.as_bytes()),
            ("manifest.json", manifest.as_bytes()),
        ] {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(output.join(name))?;
            created.push(name);
            file.write_all(bytes)?;
        }
        Ok(())
    })();
    if result.is_err() {
        // Remove only artifacts created in the directory this call created.
        for name in created {
            let _ = fs::remove_file(output.join(name));
        }
        let _ = fs::remove_dir(output);
    }
    result
}
