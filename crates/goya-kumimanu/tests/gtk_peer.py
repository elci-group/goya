"""GTK 3 selection peer for the Kumimanu interop test.

Run by tests/gtk_interop.rs with `python3 -I`. It uses a private selection
name, never CLIPBOARD, so the desktop clipboard is not touched. The PRIMARY
modes below are the one exception: PRIMARY is always the real atom, so
`own-primary` briefly borrows the desktop primary selection. That ownership
is transient — it is released when the peer exits (stdin closes or the read
finishes), and CLIPBOARD is never touched by any mode.

    own  <selection> <lines>   own text, print "ready", serve until stdin closes
    read <selection>           print the converted text, or exit 3 for none
    own-primary <lines>        own the real PRIMARY, print "ready", serve
                               until stdin closes
    read-primary               print the PRIMARY text, or exit 3 for none

The text for <lines> is `f"{i:06d}é\\n"` for i in range(lines). The Rust side
builds the same text, so neither side ships the payload in argv.
"""

import sys
import threading

import gi

gi.require_version("Gtk", "3.0")
gi.require_version("Gdk", "3.0")
from gi.repository import Gdk, GLib, Gtk  # noqa: E402


def text_for(lines: int) -> str:
    return "".join(f"{i:06d}é\n" for i in range(lines))


def clipboard(name: str) -> Gtk.Clipboard:
    if not name.startswith("GOYA_KUMIMANU_TEST_"):
        raise SystemExit("refusing a selection that is not a private test name")
    return Gtk.Clipboard.get(Gdk.Atom.intern(name, False))


def own(name: str, lines: int) -> int:
    return own_board(clipboard(name), lines)


def own_primary(lines: int) -> int:
    return own_board(Gtk.Clipboard.get(Gdk.Atom.intern("PRIMARY", False)), lines)


def own_board(board: Gtk.Clipboard, lines: int) -> int:
    board.set_text(text_for(lines), -1)

    def wait_for_eof() -> None:
        sys.stdin.read()
        GLib.idle_add(Gtk.main_quit)

    threading.Thread(target=wait_for_eof, daemon=True).start()
    print("ready", flush=True)
    Gtk.main()
    return 0


def read(name: str) -> int:
    return read_board(clipboard(name))


def read_primary() -> int:
    return read_board(Gtk.Clipboard.get(Gdk.Atom.intern("PRIMARY", False)))


def read_board(board: Gtk.Clipboard) -> int:
    text = board.wait_for_text()
    if text is None:
        return 3
    sys.stdout.buffer.write(text.encode("utf-8"))
    sys.stdout.flush()
    return 0


def main(argv: list[str]) -> int:
    if len(argv) == 4 and argv[1] == "own":
        return own(argv[2], int(argv[3]))
    if len(argv) == 3 and argv[1] == "read":
        return read(argv[2])
    if len(argv) == 3 and argv[1] == "own-primary":
        return own_primary(int(argv[2]))
    if len(argv) == 2 and argv[1] == "read-primary":
        return read_primary()
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
