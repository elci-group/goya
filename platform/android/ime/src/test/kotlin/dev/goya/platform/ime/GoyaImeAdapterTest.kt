package dev.goya.platform.ime

private class RecordingBridge(var snapshot: EditorSnapshot?) : GoyaInputBridge {
    val batches = mutableListOf<ImeBatch>()
    val selections = mutableListOf<Pair<Int, Int>>()
    val cursorRequests = mutableListOf<Pair<Int, Int>>()

    override fun editorSnapshot() = snapshot
    override fun applyImeBatch(batch: ImeBatch): Boolean {
        batches += batch
        return true
    }
    override fun setSelectionUtf8(anchor: Int, cursor: Int): Boolean {
        selections += anchor to cursor
        return true
    }
    override fun requestCursorAnchorUpdates(mode: Int, filter: Int): Boolean {
        cursorRequests += mode to filter
        return true
    }
}

/** Dependency-free checks that can be run by any Kotlin/JVM test harness. */
object GoyaImeAdapterTest {
    @JvmStatic
    fun main(args: Array<String>) {
        composingCursorUsesUtf8Bytes()
        deletionConvertsUtf16UnitsWithoutSplittingSurrogatePairs()
        commitThenMovesCaretUsingFreshTextGeometry()
        selectionAndCursorRequestsAreForwarded()
    }

    private fun composingCursorUsesUtf8Bytes() {
        val bridge = RecordingBridge(EditorSnapshot("", 0, 0))
        check(GoyaImeAdapter(bridge).setComposingText("かな", -1))
        check(bridge.batches.single().preedit == Preedit("かな", 3, 3))
    }

    private fun deletionConvertsUtf16UnitsWithoutSplittingSurrogatePairs() {
        val text = "a🧠é"
        val bridge = RecordingBridge(EditorSnapshot(text, 7, 7))
        check(GoyaImeAdapter(bridge).deleteSurroundingText(2, 0))
        val batch = bridge.batches.single()
        // Two UTF-16 units include the low surrogate and the final é. The
        // runtime expands the byte deletion to full grapheme boundaries.
        check(batch.deleteBeforeUtf8 == 6)
        check(batch.deleteAfterUtf8 == 0)
    }

    private fun commitThenMovesCaretUsingFreshTextGeometry() {
        val bridge = RecordingBridge(EditorSnapshot("ab", 1, 1))
        check(GoyaImeAdapter(bridge).commitText("é", 1))
        check(bridge.batches.single() == ImeBatch(commit = "é"))
        check(bridge.selections.single() == (3 to 3))
    }

    private fun selectionAndCursorRequestsAreForwarded() {
        val bridge = RecordingBridge(EditorSnapshot("a🧠b", 0, 0))
        val adapter = GoyaImeAdapter(bridge)
        check(adapter.setSelection(1, 3))
        check(bridge.selections.single() == (1 to 5))
        check(adapter.requestCursorUpdates(1, 16))
        check(bridge.cursorRequests.single() == (1 to 16))
    }
}
