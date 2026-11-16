use super::Definition;

pub const DEFINITIONS: &[Definition] = &[
    Definition {
        id: "network.http",
        summary: "Exchange request and response messages with an HTTP endpoint.",
        operations: &["send request", "receive response"],
        invariants: &[
            "request method and target are explicit",
            "response status and headers remain observable",
        ],
    },
    Definition {
        id: "network.websocket",
        summary: "Exchange framed messages over a negotiated WebSocket connection.",
        operations: &["open connection", "send and receive frames"],
        invariants: &[
            "connection closure is observable",
            "message ordering follows the connection",
        ],
    },
    Definition {
        id: "network.dns_lookup",
        summary: "Resolve a hostname to address records through a name-resolution service.",
        operations: &["resolve hostname", "return address records"],
        invariants: &[
            "results identify their record type",
            "resolution failure is distinguishable from an empty answer",
        ],
    },
    Definition {
        id: "network.tcp_connect",
        summary: "Establish an ordered byte stream to a network address.",
        operations: &["connect to address", "read and write bytes", "close stream"],
        invariants: &[
            "peer closure and I/O errors are observable",
            "writes preserve byte order",
        ],
    },
    Definition {
        id: "network.udp_datagram",
        summary: "Send and receive discrete datagrams addressed to network endpoints.",
        operations: &["send datagram", "receive datagram"],
        invariants: &[
            "datagram boundaries are preserved",
            "delivery and ordering are not guaranteed",
        ],
    },
    Definition {
        id: "network.tls_session",
        summary: "Negotiate an encrypted transport session with peer identity checks.",
        operations: &[
            "negotiate session",
            "inspect peer identity",
            "close session",
        ],
        invariants: &[
            "authentication failure prevents a trusted session",
            "certificate policy is explicit",
        ],
    },
    Definition {
        id: "network.proxy_route",
        summary: "Describe proxy routing for outbound network requests.",
        operations: &["select proxy route", "bypass proxy for matching target"],
        invariants: &[
            "route selection does not itself send traffic",
            "direct and proxied routes are distinguishable",
        ],
    },
    Definition {
        id: "network.listener",
        summary: "Accept incoming connections on a configured network endpoint.",
        operations: &["bind endpoint", "accept connection", "stop accepting"],
        invariants: &[
            "bound endpoint is reported",
            "accepted connections can be closed independently",
        ],
    },
    Definition {
        id: "network.timeout_policy",
        summary: "Apply bounded time limits to network connection and exchange operations.",
        operations: &["set connect timeout", "set read or write deadline"],
        invariants: &[
            "timeout units and scope are explicit",
            "timeout is reported separately from successful completion",
        ],
    },
    Definition {
        id: "storage.blob",
        summary: "Store and retrieve opaque byte objects by key.",
        operations: &["put object", "get object", "delete object"],
        invariants: &[
            "retrieved bytes match the stored object",
            "missing keys are distinguishable from empty objects",
        ],
    },
    Definition {
        id: "storage.filesystem_read",
        summary: "Read file contents and metadata from a named path.",
        operations: &["read file", "inspect file metadata"],
        invariants: &[
            "path and read failures are reported",
            "reading does not modify file contents",
        ],
    },
    Definition {
        id: "storage.filesystem_write",
        summary: "Create or replace file contents at a named path.",
        operations: &["write file", "create parent directories when requested"],
        invariants: &[
            "write completion or failure is reported",
            "replacement behavior is explicit",
        ],
    },
    Definition {
        id: "storage.object_list",
        summary: "Enumerate stored object keys using a prefix or continuation token.",
        operations: &["list keys", "continue listing"],
        invariants: &[
            "continuation does not silently skip returned entries",
            "listing order is unspecified unless declared",
        ],
    },
    Definition {
        id: "storage.metadata",
        summary: "Associate typed metadata with a stored object.",
        operations: &["read metadata", "set metadata", "remove metadata"],
        invariants: &[
            "metadata keys and values are explicit",
            "metadata updates do not imply object byte changes",
        ],
    },
    Definition {
        id: "storage.transaction",
        summary: "Group supported storage changes into an all-or-nothing transaction.",
        operations: &["begin transaction", "commit changes", "abort changes"],
        invariants: &[
            "commit or abort outcome is observable",
            "unsupported transactional operations are rejected",
        ],
    },
    Definition {
        id: "storage.snapshot",
        summary: "Create and access a stable point-in-time view of stored state.",
        operations: &["create snapshot", "read snapshot", "release snapshot"],
        invariants: &[
            "snapshot identity is explicit",
            "snapshot reads do not mutate the captured state",
        ],
    },
    Definition {
        id: "storage.quota",
        summary: "Report and enforce declared storage capacity limits.",
        operations: &["inspect usage", "set capacity limit"],
        invariants: &[
            "reported usage identifies its scope",
            "capacity rejection is reported before claiming success",
        ],
    },
    Definition {
        id: "storage.retention",
        summary: "Describe expiry and retention rules for stored objects.",
        operations: &[
            "set expiry policy",
            "inspect expiry",
            "remove expired object",
        ],
        invariants: &[
            "expiry timestamps include a defined time basis",
            "retention policy does not promise undeclared recovery",
        ],
    },
    Definition {
        id: "data.json",
        summary: "Parse and emit JSON values with explicit parse errors.",
        operations: &["parse JSON", "encode JSON"],
        invariants: &[
            "invalid input is not silently accepted",
            "object member ordering is not guaranteed",
        ],
    },
    Definition {
        id: "data.csv",
        summary: "Parse and emit delimited tabular records with declared dialect rules.",
        operations: &["parse rows", "encode rows"],
        invariants: &[
            "delimiter, quoting, and header policy are explicit",
            "record boundaries survive quoted delimiters",
        ],
    },
    Definition {
        id: "data.binary_codec",
        summary: "Encode and decode values using a declared binary format.",
        operations: &["encode value", "decode bytes"],
        invariants: &[
            "format identity and version are explicit",
            "malformed or unsupported input is reported",
        ],
    },
    Definition {
        id: "data.stream",
        summary: "Transfer a sequence of data chunks with bounded incremental access.",
        operations: &["read next chunk", "write chunk", "finish stream"],
        invariants: &[
            "chunk order is preserved",
            "end-of-stream is distinct from read failure",
        ],
    },
    Definition {
        id: "data.compression",
        summary: "Compress and decompress byte sequences using a declared codec.",
        operations: &["compress bytes", "decompress bytes"],
        invariants: &[
            "codec identity is explicit",
            "decompression errors are reported without claiming valid output",
        ],
    },
    Definition {
        id: "data.checksum",
        summary: "Compute and compare a digest for a byte sequence.",
        operations: &["compute digest", "verify digest"],
        invariants: &[
            "algorithm identity is explicit",
            "digest verification does not establish authenticity",
        ],
    },
    Definition {
        id: "data.schema_validation",
        summary: "Check structured data against a supplied schema.",
        operations: &["validate value", "report violations"],
        invariants: &[
            "schema identity is explicit",
            "validation does not rewrite the input",
        ],
    },
    Definition {
        id: "data.import_records",
        summary: "Convert external records into a declared internal record shape.",
        operations: &[
            "read source records",
            "map fields",
            "report rejected records",
        ],
        invariants: &[
            "mapping rules are explicit",
            "rejected records remain distinguishable from omitted fields",
        ],
    },
    Definition {
        id: "data.export_records",
        summary: "Convert internal records into a declared external record shape.",
        operations: &["select fields", "map records", "emit output records"],
        invariants: &[
            "field selection and mapping are explicit",
            "output record order is preserved when input order is supplied",
        ],
    },
];
