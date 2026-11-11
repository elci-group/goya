# Native frontend accessibility contract

The same frame supplies pointer hit regions and semantic control rectangles. A single `A11yTree` snapshot carries both structural semantics and current composer text; separate debug-node patches must not diverge from the tree exported to assistive technology. `A11yTree::set_live_text` updates an existing text input only, returns false for missing/non-text nodes, and never creates nodes. Empty values restore placeholder announcement.

Workflow buttons are exported with the same stable IDs, labels and enabled states as `SessionView::workflow_controls()`. Controls absent from the current scene are absent from the semantic tree. Hidden or unavailable workflow controls must not expose actionable ghost buttons. Session navigation/send controls retain the existing authoritative route; semantic activation must share the UI action handler.

Regression: exported tree resolves typed composer text; clearing restores the placeholder, a missing ID does not mutate the tree, and workflow controls preserve enabled/disabled state and actual frame geometry. These checks establish semantic consistency. Live AT-SPI/operator, IME and other-platform qualification remain separate gates; a debug snapshot does not establish working screen-reader interaction.

Browser navigation keys are opt-in per native window. Opted-in browser windows receive a typed BrowserNavigation event for Alt+Left/Right, Ctrl+R and F5. Existing session shortcuts and legacy windows retain their prior semantics. Alt is read from the active XKB modifier map; untracked modifier guesses do not produce navigation.

## Live native accessibility mirror

The optional `atspi` feature builds `zcode-a11y-bridge`, an isolated native GTK/AT-SPI process. The Goya host sends bounded semantic snapshots over private child stdin and receives typed actions over stdout. GTK runs in the helper's main thread; the Wayland window retains its own event loop. Each snapshot replaces display semantics only. Actions return to the same native workflow dispatcher, never directly to services. The helper owns no task/session facts or accepted queue.

The wire caps snapshots at 1 MiB/512 nodes and actions at 64 KiB; IDs are unique and parent references must form an acyclic rooted tree. The parent keeps only the latest unsent snapshot and bounded actions. Disconnect closes the mirror, and parent shutdown kills/waits its owned helper and joins I/O workers. Disabled/missing semantic controls cannot invoke actions. Failure to start the optional bridge is visible rather than a claim of working accessibility. This development mirror has a separate native window; direct accessibility export from the Goya visual surface and operator qualification remain release requirements.

The GTK mirror resolves current text from live text first, then formal value, then empty text. Clearing a value updates the widget; stale entry text must not survive an empty projection.
