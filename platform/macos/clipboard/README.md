# macOS clipboard adapter

This isolated AppKit adapter provides explicit plain-text copy, cut and paste
entry points. It is not wired into a Goya host. Call only on the main thread
from an admitted user action. The owner must reserve and revalidate the target,
policy, focus, identity, epoch, revision and selection in `admit`, and validate
and atomically commit in `commit`. A cut reports partial export when clipboard
write succeeds but revalidation or local commit fails; it does not compensate
by rewriting the clipboard. Export failure can also leave the clipboard cleared.

An admitted copy or cut of empty text returns `GoyaClipboardNoChange` without
touching the pasteboard or calling the commit callback. Exports must be valid
UTF-8 and fit both the caller's nonzero byte limit and the profile's 1 MiB
maximum; their encoded length is checked before allocating the transfer data.
Invalid text and oversized exports leave the clipboard and local text unchanged.

Paste returns unavailable without accessing the pasteboard. AppKit materializes
pasteboard data before its size can be checked, so bounded incremental decoding
is not established. The owner is responsible for routing all
native actions through policy and suppressing denied event propagation.

Build this file with ARC and the AppKit framework in a macOS host. No native
route, accessibility, privacy, interoperability or device testing is implied.
