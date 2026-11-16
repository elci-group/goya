//! Deterministic construction of a small HTTPS GET capability.
use std::fmt;

pub const CONTRACT: &str = "goya.network.fetch.v1";
pub const RECIPE_VERSION: &str = "0.0.1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    Browser,
    Node,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NetworkDeclaration {
    pub timeout_ms: u32,
    pub concurrency: u16,
    pub retries: u16,
    pub offline_queue: bool,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NetworkIr {
    timeout_ms: u32,
    concurrency: u16,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic(pub &'static str);
impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for Diagnostic {}

impl NetworkDeclaration {
    pub fn normalize(self) -> Result<NetworkIr, Diagnostic> {
        if self.timeout_ms == 0 || self.timeout_ms > i32::MAX as u32 {
            return Err(Diagnostic(
                "timeout must be between 1 and 2147483647 milliseconds",
            ));
        }
        if self.concurrency == 0 {
            return Err(Diagnostic("concurrency must be positive"));
        }
        if self.retries != 0 {
            return Err(Diagnostic("retries are not supported"));
        }
        if self.offline_queue {
            return Err(Diagnostic("offline queue is not supported"));
        }
        Ok(NetworkIr {
            timeout_ms: self.timeout_ms,
            concurrency: self.concurrency,
        })
    }
}

impl NetworkIr {
    pub fn canonical(&self) -> String {
        format!(
            "contract={CONTRACT}\ntimeout_ms={}\nconcurrency={}\n",
            self.timeout_ms, self.concurrency
        )
    }
    pub fn lower(&self, target: Target) -> Artifact {
        let recipe = match target {
            Target::Browser => "browser-fetch",
            Target::Node => "node-fetch",
        };
        let source = format!(
            r#"// {CONTRACT}; recipe={recipe}@{RECIPE_VERSION}
const limit = {concurrency};
const timeoutMs = {timeout};
let active = 0;
const pending = [];
function release() {{
  if (pending.length) pending.shift()();
  else active--;
}}
function acquire() {{
  if (active < limit) {{ active++; return Promise.resolve(); }}
  return new Promise(resolve => pending.push(resolve));
}}
export async function get(url) {{
  const parsed = new URL(url);
  if (parsed.protocol !== 'https:') throw new TypeError('HTTPS required');
  await acquire();
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  try {{
    const response = await fetch(parsed, {{ method: 'GET', redirect: 'error', signal: controller.signal }});
    const body = new Uint8Array(await response.arrayBuffer());
    return {{ status: response.status, body }};
  }} finally {{
    clearTimeout(timer);
    release();
  }}
}}
"#,
            concurrency = self.concurrency,
            timeout = self.timeout_ms
        );
        Artifact {
            contract: CONTRACT,
            recipe,
            recipe_version: RECIPE_VERSION,
            canonical_ir: self.canonical(),
            source,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Artifact {
    pub contract: &'static str,
    pub recipe: &'static str,
    pub recipe_version: &'static str,
    pub canonical_ir: String,
    pub source: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn declaration() -> NetworkDeclaration {
        NetworkDeclaration {
            timeout_ms: 5000,
            concurrency: 8,
            retries: 0,
            offline_queue: false,
        }
    }
    #[test]
    fn canonical_and_lowering_are_stable() {
        let ir = declaration().normalize().unwrap();
        assert_eq!(
            ir.canonical(),
            "contract=goya.network.fetch.v1\ntimeout_ms=5000\nconcurrency=8\n"
        );
        for target in [Target::Browser, Target::Node] {
            assert_eq!(ir.lower(target), ir.lower(target));
            assert!(ir.lower(target).source.contains("redirect: 'error'"));
        }
        assert_ne!(
            ir.lower(Target::Browser).recipe,
            ir.lower(Target::Node).recipe
        );
    }
    #[test]
    fn unsupported_requirements_fail_closed() {
        let mut d = declaration();
        d.retries = 1;
        assert!(d.normalize().is_err());
        d = declaration();
        d.offline_queue = true;
        assert!(d.normalize().is_err());
        d = declaration();
        d.concurrency = 0;
        assert!(d.normalize().is_err());
        d = declaration();
        d.timeout_ms = 0;
        assert!(d.normalize().is_err());
        d.timeout_ms = u32::MAX;
        assert!(d.normalize().is_err());
    }
}
