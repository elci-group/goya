# Goya parity integration gates

Spec: DROWZY-GUI-PARITY-001. These checks establish only their explicit protocol/GUI foundation slice, not full parity.

`tools/verify-zcode-fixtures.mts` validates synthetic JSON fixtures using the actual schemas in a supplied Drowzy checkout. Each manifest entry names a supported exported schema and expected validity. Unknown schema names, unreadable files, validation mismatches, missing positive/negative coverage and zero cases fail. Commands require payload validation as well as envelope validation. Async file IO is required. No credentials, live session content or paid model execution are used.

`parity.deliver.toml` requires implementation/spec/tooling files and runs the targeted Rust regressions plus monitor headless acceptance. Missing artifacts and nonzero checks fail. Root additionally runs full workspace gates and records revision, dirty state, environment, commands and exit results. A fixture/reducer/window-presentation result cannot certify Host attachment, real IME, screen readers or clipboard interoperability.

Ownership: GUI draft → adapter command → existing runtime admission → authoritative projection → GUI. Fixture validation never mutates live state. Desktop continuous and phone replayable need separate integration evidence.
