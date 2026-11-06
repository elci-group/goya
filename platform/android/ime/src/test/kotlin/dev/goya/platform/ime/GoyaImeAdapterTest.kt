package dev.goya.platform.ime

private class RecordingBridge(var snapshot: EditorSnapshot?) : GoyaInputBridge {
    val batches = mutableListOf<ImeBatch>()
    val selections = mutableListOf<Pair<Int, Int>>()
    val composingRegions = mutableListOf<Pair<Int, Int>>()
    val committedTexts = mutableListOf<Pair<String, Int>>()
    val deletions = mutableListOf<Pair<Int, Int>>()
    val cursorRequests = mutableListOf<Pair<Int, Int>>()
    var finishedCompositions = 0
    var snapshotReads = 0

    override fun editorSnapshot(): EditorSnapshot? {
        snapshotReads += 1
        return snapshot
    }
    override fun applyImeBatch(batch: ImeBatch): Boolean {
        batches += batch
        return true
    }
    override fun setSelectionUtf8(anchor: Int, cursor: Int): Boolean {
        selections += anchor to cursor
        return true
    }
    override fun setComposingRegionUtf8(start: Int, end: Int): Boolean {
        composingRegions += start to end
        return true
    }
    override fun commitInputText(text: String, cursorByteInText: Int): Boolean {
        committedTexts += text to cursorByteInText
        return true
    }
    override fun deleteInputSurrounding(beforeBytes: Int, afterBytes: Int): Boolean {
        deletions += beforeBytes to afterBytes
        return true
    }
    override fun finishComposition(): Boolean {
        finishedCompositions += 1
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
        deletionPreservesSelectionAndUsesBothEndpoints()
        commitIsAtomicAndDoesNotReadASnapshot()
        selectionAndCursorRequestsAreForwarded()
        composingRegionsNormalizeAndFinish()
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

    private fun deletionPreservesSelectionAndUsesBothEndpoints() {
        val bridge = RecordingBridge(EditorSnapshot("a🧠b", 5, 1))
        check(GoyaImeAdapter(bridge).deleteSurroundingText(1, 1))
        // Selection is bytes 1..5. Delete one UTF-16 char before its start
        // and after its end; the runtime event preserves the selected range.
        check(bridge.deletions.single() == (1 to 1))
    }

    private fun commitIsAtomicAndDoesNotReadASnapshot() {
        val bridge = RecordingBridge(EditorSnapshot("ab", 1, 1))
        check(GoyaImeAdapter(bridge).commitText("🧠é", 1))
        check(bridge.committedTexts.single() == ("🧠é" to 6))
        check(bridge.snapshotReads == 0)
        check(bridge.selections.isEmpty())
        check(bridge.batches.isEmpty())
    }

    private fun selectionAndCursorRequestsAreForwarded() {
        val bridge = RecordingBridge(EditorSnapshot("a🧠b", 0, 0))
        val adapter = GoyaImeAdapter(bridge)
        check(adapter.setSelection(1, 3))
        check(bridge.selections.single() == (1 to 5))
        check(adapter.requestCursorUpdates(1, 16))
        check(bridge.cursorRequests.single() == (1 to 16))
    }

    private fun composingRegionsNormalizeAndFinish() {
        val bridge = RecordingBridge(EditorSnapshot("a🧠b", 5, 5))
        val adapter = GoyaImeAdapter(bridge)
        check(adapter.setComposingRegion(3, 1))
        check(bridge.composingRegions.single() == (1 to 5))
        check(bridge.selections.isEmpty()) // composing span does not move selection

        check(adapter.setComposingRegion(2, 2)) // inside surrogate pair snaps to its start
        check(bridge.finishedCompositions == 1)
        check(adapter.finishComposingText())
        check(bridge.finishedCompositions == 2)
    }
}
