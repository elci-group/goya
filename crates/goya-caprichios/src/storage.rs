//! Best-effort single-key byte storage with an IndexedDB browser adapter.
//!
//! Generated modules export `get(key)`, `put(key, bytes)`, and `deleteKey(key)`.
//! Keys must be strings; values must be Uint8Array instances. `get` returns a copied
//! Uint8Array or undefined for a missing key. Writes snapshot input bytes before any
//! asynchronous work. Oversize values reject with RangeError, invalid arguments with
//! TypeError. IndexedDB open/request/transaction errors propagate as promise rejections.
//! Every operation waits for its transaction to commit; deletion of a missing key succeeds.
//! Each namespace owns a separate database. No eviction, quota, cross-key transaction,
//! encryption, or persistence guarantee is provided; successful commit is best-effort.
use std::fmt;

pub const CONTRACT: &str = "goya.storage.kv.v1";
pub const RECIPE_VERSION: &str = "0.0.1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Durability {
    /// Data may be evicted or lost by the host; this is not a persistence guarantee.
    BestEffort,
    Durable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StorageDeclaration {
    pub namespace: String,
    /// Maximum accepted value size, not a reservation of host storage capacity.
    pub max_value_bytes: u32,
    pub durability: Durability,
    pub multi_key_transactions: bool,
    pub encryption_at_rest: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StorageIr {
    namespace: String,
    max_value_bytes: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    Browser,
    Node,
    Native,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RuntimeCapabilities {
    /// IndexedDB must be available in the actual execution context.
    pub indexed_db: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic(pub &'static str);

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for Diagnostic {}

/// A negotiated design only: this type contains no executable implementation.
/// Successful negotiation does not establish storage availability, quota, or durability.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoragePlan {
    pub contract: &'static str,
    pub proposed_recipe: &'static str,
    pub canonical_ir: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StorageArtifact {
    pub contract: &'static str,
    pub recipe: &'static str,
    pub recipe_version: &'static str,
    pub canonical_ir: String,
    pub source: String,
}

impl StorageDeclaration {
    pub fn normalize(self) -> Result<StorageIr, Diagnostic> {
        // Restrict the namespace alphabet so canonical lines are unambiguous and portable.
        if self.namespace.is_empty()
            || self.namespace.len() > 128
            || !self
                .namespace
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
        {
            return Err(Diagnostic("namespace must contain 1 to 128 ASCII letters, digits, dots, underscores, or hyphens"));
        }
        if self.max_value_bytes == 0 {
            return Err(Diagnostic("maximum value size must be positive"));
        }
        if self.durability != Durability::BestEffort {
            return Err(Diagnostic("durable storage is not supported"));
        }
        if self.multi_key_transactions {
            return Err(Diagnostic("multi-key transactions are not supported"));
        }
        if self.encryption_at_rest {
            return Err(Diagnostic("encryption at rest is not supported"));
        }
        Ok(StorageIr {
            namespace: self.namespace,
            max_value_bytes: self.max_value_bytes,
        })
    }
}

impl StorageIr {
    /// Lower only a negotiated browser environment. There is no fallback backend.
    pub fn lower(
        &self,
        target: Target,
        capabilities: RuntimeCapabilities,
    ) -> Result<StorageArtifact, Diagnostic> {
        let plan = self.negotiate(target, capabilities)?;
        let source = format!(
            r#"// {CONTRACT}; recipe=browser-indexeddb@{RECIPE_VERSION}
const databaseName = '{CONTRACT}:{namespace}';
const maxValueBytes = {max_value_bytes};
function checkKey(key) {{
  if (typeof key !== 'string') throw new TypeError('storage key must be a string');
}}
function openDatabase() {{
  return new Promise((resolve, reject) => {{
    let settled = false;
    const request = indexedDB.open(databaseName, 1);
    request.onupgradeneeded = () => {{ request.result.createObjectStore('values'); }};
    request.onerror = () => {{ settled = true; reject(request.error); }};
    request.onblocked = () => {{ settled = true; reject(new Error('storage database upgrade blocked')); }};
    request.onsuccess = () => {{
      const db = request.result;
      if (settled) {{ db.close(); return; }}
      db.onversionchange = () => db.close();
      resolve(db);
    }};
  }});
}}
async function operate(mode, action) {{
  const db = await openDatabase();
  return new Promise((resolve, reject) => {{
    let tx;
    let result;
    let failure;
    try {{
      tx = db.transaction('values', mode);
      tx.oncomplete = () => {{ db.close(); resolve(result); }};
      tx.onabort = () => {{ db.close(); reject(failure || tx.error || new Error('storage transaction aborted')); }};
      const request = action(tx.objectStore('values'));
      request.onerror = () => {{ failure = request.error; }};
      request.onsuccess = () => {{ result = request.result; }};
    }} catch (error) {{
      if (tx) {{ try {{ tx.abort(); }} catch (_) {{}} }}
      db.close();
      reject(error);
    }}
  }});
}}
export async function get(key) {{
  checkKey(key);
  const value = await operate('readonly', store => store.get(key));
  if (value === undefined) return undefined;
  if (!(value instanceof Uint8Array)) throw new TypeError('stored value must be Uint8Array');
  if (value.byteLength > maxValueBytes) throw new RangeError('stored value exceeds maximum size');
  return new Uint8Array(value);
}}
export async function put(key, bytes) {{
  checkKey(key);
  if (!(bytes instanceof Uint8Array)) throw new TypeError('storage value must be Uint8Array');
  if (bytes.byteLength > maxValueBytes) throw new RangeError('storage value exceeds maximum size');
  const snapshot = new Uint8Array(bytes);
  await operate('readwrite', store => store.put(snapshot, key));
}}
export async function deleteKey(key) {{
  checkKey(key);
  await operate('readwrite', store => store.delete(key));
}}
"#,
            namespace = self.namespace,
            max_value_bytes = self.max_value_bytes
        );
        Ok(StorageArtifact {
            contract: CONTRACT,
            recipe: plan.proposed_recipe,
            recipe_version: RECIPE_VERSION,
            canonical_ir: plan.canonical_ir,
            source,
        })
    }

    pub fn canonical(&self) -> String {
        format!("contract={CONTRACT}\nnamespace={}\nmax_value_bytes={}\ndurability=best-effort\nmulti_key_transactions=false\nencryption_at_rest=false\n", self.namespace, self.max_value_bytes)
    }

    pub fn negotiate(
        &self,
        target: Target,
        capabilities: RuntimeCapabilities,
    ) -> Result<StoragePlan, Diagnostic> {
        match target {
            Target::Node => return Err(Diagnostic("Node storage recipe is not supported")),
            Target::Native => return Err(Diagnostic("native storage recipe is not supported")),
            Target::Browser if !capabilities.indexed_db => {
                return Err(Diagnostic("target runtime requires IndexedDB"))
            }
            Target::Browser => {}
        }
        Ok(StoragePlan {
            contract: CONTRACT,
            proposed_recipe: "browser-indexeddb",
            canonical_ir: self.canonical(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declaration() -> StorageDeclaration {
        StorageDeclaration {
            namespace: "app.settings-v1".into(),
            max_value_bytes: 4096,
            durability: Durability::BestEffort,
            multi_key_transactions: false,
            encryption_at_rest: false,
        }
    }

    #[test]
    fn canonical_form_records_every_guarantee() {
        let ir = declaration().normalize().unwrap();
        assert_eq!(ir.canonical(), "contract=goya.storage.kv.v1\nnamespace=app.settings-v1\nmax_value_bytes=4096\ndurability=best-effort\nmulti_key_transactions=false\nencryption_at_rest=false\n");
        let capabilities = RuntimeCapabilities { indexed_db: true };
        let plan = ir.negotiate(Target::Browser, capabilities).unwrap();
        assert_eq!(plan.canonical_ir, ir.canonical());
        assert_eq!(plan.proposed_recipe, "browser-indexeddb");
        assert_eq!(plan, ir.negotiate(Target::Browser, capabilities).unwrap());
        let mut changed = declaration();
        changed.max_value_bytes += 1;
        assert_ne!(ir.canonical(), changed.normalize().unwrap().canonical());
    }

    #[test]
    fn invalid_names_cannot_inject_or_alias_canonical_fields() {
        for namespace in [
            "",
            "app\nmax_value_bytes=1",
            "white space",
            "é",
            "a/b",
            &"a".repeat(129),
        ] {
            let mut d = declaration();
            d.namespace = namespace.into();
            assert!(d.normalize().is_err(), "accepted namespace {namespace:?}");
        }
        let mut d = declaration();
        d.namespace = "a".repeat(128);
        assert!(d.normalize().is_ok());
    }

    #[test]
    fn unsupported_guarantees_fail_closed() {
        let mut d = declaration();
        d.durability = Durability::Durable;
        assert_eq!(
            d.normalize(),
            Err(Diagnostic("durable storage is not supported"))
        );
        let mut d = declaration();
        d.multi_key_transactions = true;
        assert_eq!(
            d.normalize(),
            Err(Diagnostic("multi-key transactions are not supported"))
        );
        let mut d = declaration();
        d.encryption_at_rest = true;
        assert_eq!(
            d.normalize(),
            Err(Diagnostic("encryption at rest is not supported"))
        );
        let mut d = declaration();
        d.max_value_bytes = 0;
        assert_eq!(
            d.normalize(),
            Err(Diagnostic("maximum value size must be positive"))
        );
    }

    #[test]
    fn capabilities_do_not_enable_unimplemented_targets() {
        let ir = declaration().normalize().unwrap();
        assert_eq!(
            ir.negotiate(Target::Browser, RuntimeCapabilities::default()),
            Err(Diagnostic("target runtime requires IndexedDB"))
        );
        for indexed_db in [false, true] {
            let caps = RuntimeCapabilities { indexed_db };
            assert_eq!(
                ir.negotiate(Target::Node, caps),
                Err(Diagnostic("Node storage recipe is not supported"))
            );
            assert_eq!(
                ir.negotiate(Target::Native, caps),
                Err(Diagnostic("native storage recipe is not supported"))
            );
        }
    }

    #[test]
    fn lowering_requires_negotiation_and_binds_namespace_and_limit() {
        let ir = declaration().normalize().unwrap();
        let caps = RuntimeCapabilities { indexed_db: true };
        let artifact = ir.lower(Target::Browser, caps).unwrap();
        assert_eq!(artifact, ir.lower(Target::Browser, caps).unwrap());
        assert_eq!(artifact.canonical_ir, ir.canonical());
        assert!(artifact
            .source
            .contains("goya.storage.kv.v1:app.settings-v1"));
        assert!(artifact.source.contains("const maxValueBytes = 4096;"));
        assert!(ir
            .lower(Target::Browser, RuntimeCapabilities::default())
            .is_err());
        assert!(ir.lower(Target::Node, caps).is_err());
        assert!(ir.lower(Target::Native, caps).is_err());
        let mut other = declaration();
        other.namespace = "another".into();
        assert_ne!(
            artifact.source,
            other
                .normalize()
                .unwrap()
                .lower(Target::Browser, caps)
                .unwrap()
                .source
        );
    }
}
