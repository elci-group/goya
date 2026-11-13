# iOS Gestures clipboard slice

`GoyaClipboard.swift` is an isolated UIKit plain-text export primitive. It is
not wired into a control or an application. A host must invoke `copy` only from
an explicit user action and synchronously supply a selection after checking the
current principal, focus, selection, policy and protected-content restrictions.
Returning nil denies the operation. The host must also block every other native
export route; this primitive does not intercept menus, shortcuts, accessibility
actions or direct framework calls. An unchecked callback is not authorization.

The UTF-8 export limit is provisional (1 MiB). The pasteboard write replaces its
items with one UTF-8 plain-text representation. `exported` means the UIKit setter
was invoked; it is not an acknowledgement of persistence or receipt. The API
does not log content or query the pasteboard. Paste and cut always return
`unsupported` without touching the clipboard or editor: UIKit import has not
been shown to provide bounded incremental decoding, and no editor reservation,
revalidation or observable export acknowledgement is available here.

The file requires UIKit and UniformTypeIdentifiers (iOS 14 or later) and an
Apple Swift toolchain. It has not been compiled or exercised on an iOS device.
There is no native route coverage, accessibility evidence, policy integration,
or target qualification. The existing Gestures admission gates remain blocked.
