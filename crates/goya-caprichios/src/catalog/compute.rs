use super::Definition;

pub const DEFINITIONS: &[Definition] = &[
    Definition {
        id: "compute.tasks",
        summary: "Schedule bounded units of work with explicit completion and failure outcomes.",
        operations: &["submit task", "cancel task", "observe task result"],
        invariants: &[
            "Each accepted task has one terminal outcome.",
            "Cancellation does not imply that already performed side effects are undone.",
        ],
    },
    Definition {
        id: "compute.parallelism",
        summary: "Describe independent work that may execute concurrently under a declared limit.",
        operations: &["set concurrency limit", "run independent work items"],
        invariants: &[
            "The declared limit is an upper bound on simultaneous work.",
            "Concurrent work may complete in an order different from submission.",
        ],
    },
    Definition {
        id: "compute.deadlines",
        summary: "Associate work with a deadline and report when its allowed time expires.",
        operations: &["set deadline", "check remaining time", "report expiry"],
        invariants: &[
            "A deadline bounds waiting or execution only where the host can enforce it.",
            "Expiry does not guarantee interruption of non-cooperative work.",
        ],
    },
    Definition {
        id: "compute.retry_policy",
        summary: "Describe bounded retry behavior for operations whose failure may be transient.",
        operations: &[
            "set attempt limit",
            "classify retryable failure",
            "compute retry delay",
        ],
        invariants: &[
            "Attempts never exceed the declared limit.",
            "Retrying a non-idempotent operation can repeat its side effects.",
        ],
    },
    Definition {
        id: "compute.work_queue",
        summary: "Represent queued work with explicit admission and dequeue behavior.",
        operations: &[
            "enqueue work",
            "dequeue eligible work",
            "report queue depth",
        ],
        invariants: &[
            "Queue capacity is finite when a capacity is declared.",
            "Admission failure leaves the rejected item unaccepted.",
        ],
    },
    Definition {
        id: "compute.priorities",
        summary: "Assign relative scheduling preference to work items.",
        operations: &["assign priority", "select eligible work", "change priority"],
        invariants: &[
            "Priority expresses preference and does not guarantee a start time.",
            "Priority cannot make ineligible work runnable.",
        ],
    },
    Definition {
        id: "compute.batching",
        summary: "Group compatible work items for joint processing under a declared batch bound.",
        operations: &["add item to batch", "flush batch", "set batch bound"],
        invariants: &[
            "A batch does not exceed its declared item bound.",
            "Batching does not imply atomic processing of its items.",
        ],
    },
    Definition {
        id: "compute.progress",
        summary: "Report monotonic progress for work with a known measurable total.",
        operations: &[
            "report completed units",
            "report total units",
            "mark completion",
        ],
        invariants: &[
            "Reported completed units do not decrease within a task.",
            "Progress is an estimate unless the work defines an exact total.",
        ],
    },
    Definition {
        id: "compute.isolation",
        summary: "Describe boundaries between independently scheduled work contexts.",
        operations: &[
            "create execution context",
            "assign work to context",
            "release context",
        ],
        invariants: &[
            "A context boundary alone does not guarantee process or memory isolation.",
            "Resources remain subject to host scheduling and limits.",
        ],
    },
    Definition {
        id: "memory.buffers",
        summary: "Allocate and use byte buffers with an explicit capacity and valid length.",
        operations: &["allocate buffer", "read byte range", "write byte range"],
        invariants: &[
            "Access ranges stay within the allocated capacity.",
            "Capacity is not a promise of physical memory availability.",
        ],
    },
    Definition {
        id: "memory.ownership",
        summary: "Describe which component may read or modify a memory value.",
        operations: &["transfer ownership", "borrow value", "release value"],
        invariants: &[
            "A transferred value has one declared owner at a time.",
            "Ownership metadata does not itself prevent host-level aliasing.",
        ],
    },
    Definition {
        id: "memory.shared_views",
        summary: "Expose bounded views over shared memory with explicit access rules.",
        operations: &["create view", "read shared range", "write shared range"],
        invariants: &[
            "Every view range lies within its backing region.",
            "Concurrent access requires coordination to avoid data races.",
        ],
    },
    Definition {
        id: "memory.limits",
        summary: "Declare a maximum memory budget for a workload or component.",
        operations: &[
            "set memory budget",
            "account allocation",
            "report limit breach",
        ],
        invariants: &[
            "A declared budget is a constraint only when enforced by the host.",
            "Accounting units and scope must be consistent within a declaration.",
        ],
    },
    Definition {
        id: "memory.lifetimes",
        summary: "Describe the period during which allocated values remain valid.",
        operations: &["begin lifetime", "use value", "end lifetime"],
        invariants: &[
            "A value is not valid after its declared lifetime ends.",
            "Ending a logical lifetime does not promise immediate reclamation.",
        ],
    },
    Definition {
        id: "memory.reuse",
        summary: "Reuse released capacity for compatible allocations.",
        operations: &["release region", "request reusable region", "reset region"],
        invariants: &[
            "A region is reused only after its prior use is released.",
            "Reused contents are unspecified unless explicitly initialized.",
        ],
    },
    Definition {
        id: "memory.serialization",
        summary: "Convert values to and from an explicitly selected byte representation.",
        operations: &["encode value", "decode bytes", "validate encoded length"],
        invariants: &[
            "Decoding rejects inputs that violate the selected format.",
            "Round trips preserve only information represented by that format.",
        ],
    },
    Definition {
        id: "memory.zeroing",
        summary: "Request initialization or clearing of a memory range before reuse.",
        operations: &["initialize range", "clear range", "verify range length"],
        invariants: &[
            "The requested range is bounded by its allocation.",
            "Clearing managed memory does not guarantee removal of copies outside that range.",
        ],
    },
    Definition {
        id: "memory.pressure",
        summary: "Observe memory pressure and define a response when available capacity is low.",
        operations: &[
            "observe pressure",
            "set pressure threshold",
            "request workload reduction",
        ],
        invariants: &[
            "Pressure signals are advisory unless the host defines enforcement.",
            "A response cannot assume that memory can always be reclaimed immediately.",
        ],
    },
    Definition {
        id: "security.identity",
        summary: "Represent a principal and the evidence used to establish its identity.",
        operations: &[
            "identify principal",
            "validate identity evidence",
            "expire identity",
        ],
        invariants: &[
            "An asserted identity is distinct from verified identity.",
            "Identity evidence is valid only within its declared scope and lifetime.",
        ],
    },
    Definition {
        id: "security.authorization",
        summary: "Evaluate whether a principal may perform a named action on a resource.",
        operations: &[
            "declare permission",
            "authorize action",
            "revoke permission",
        ],
        invariants: &[
            "An action is allowed only when the applicable policy grants it.",
            "Authorization decisions do not establish that the caller is trustworthy.",
        ],
    },
    Definition {
        id: "security.secrets",
        summary: "Handle sensitive values with restricted exposure and explicit lifetime.",
        operations: &[
            "provide secret reference",
            "use secret for operation",
            "expire secret access",
        ],
        invariants: &[
            "Secret values are not included in ordinary diagnostic output.",
            "A reference does not guarantee secure storage by the host.",
        ],
    },
    Definition {
        id: "security.input_validation",
        summary: "Validate untrusted values against declared shape and size constraints.",
        operations: &[
            "declare input constraints",
            "validate input",
            "reject invalid input",
        ],
        invariants: &[
            "Invalid input is rejected before the protected operation proceeds.",
            "Validation applies only to the representation actually checked.",
        ],
    },
    Definition {
        id: "security.audit_events",
        summary: "Record security-relevant decisions as structured events for later review.",
        operations: &[
            "record decision",
            "attach event context",
            "query recorded events",
        ],
        invariants: &[
            "Events distinguish observed facts from inferred context.",
            "Recording does not guarantee retention or tamper resistance.",
        ],
    },
    Definition {
        id: "security.transport_protection",
        summary: "Declare protection expectations for data exchanged over a transport.",
        operations: &[
            "select protected transport",
            "validate peer",
            "report protection status",
        ],
        invariants: &[
            "Protection requires support from both endpoints and the transport.",
            "A protected channel does not validate application-level data.",
        ],
    },
    Definition {
        id: "security.integrity_checks",
        summary: "Check data against an expected digest or integrity value.",
        operations: &[
            "compute digest",
            "compare expected value",
            "report mismatch",
        ],
        invariants: &[
            "Integrity checks detect only changes relative to a trusted expected value.",
            "A digest alone does not establish authenticity.",
        ],
    },
    Definition {
        id: "security.resource_limits",
        summary: "Bound resource use to reduce the impact of excessive or hostile workloads.",
        operations: &[
            "declare resource quota",
            "account consumption",
            "reject quota excess",
        ],
        invariants: &[
            "A quota is effective only when enforced at the relevant resource boundary.",
            "Quota rejection does not undo resource use that already occurred.",
        ],
    },
    Definition {
        id: "security.capability_scoping",
        summary: "Limit an authority token to named actions, resources, and lifetime.",
        operations: &[
            "issue scoped capability",
            "check capability",
            "revoke capability",
        ],
        invariants: &[
            "A capability grants no authority beyond its declared scope.",
            "Revocation takes effect only where the receiving system checks it.",
        ],
    },
];
