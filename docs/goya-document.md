# Native document page adapter

Contract GY-DOCUMENT/1, initial implementation slice of DROWZY-GUI-PARITY-002.

The document worker owns one PDF page render at a time. UI submits an explicit local path/page/maximum side and receives bounded opaque RGBA. The worker never owns task/session state. PDF parser execution is Linux-only through installed bwrap, prlimit and pdftoppm with a read-only input mount, no network, private temporary runtime, CPU/address-space limits and a wall deadline. Missing sandbox/renderer is an error; there is no unsandboxed fallback.

P6 PPM decoding is a pure boundary: nonzero dimensions, maximum side/pixel count, maxval=255, exact raster length, no trailing bytes, and correct binary whitespace handling. The adapter expands RGB to opaque RGBA and bounds input/output before allocation. Rendering is a blocking worker operation, never a UI-loop operation. The native CLI viewer presents the result with the shared goya-wayland pixel surface API and keeps renderer work off its event loop.

Acceptance: decoder corruption/overflow/header/raster tests; argument/invocation tests; opt-in synthetic PDF actual Poppler render and text extraction in sandbox; native window presentation. `extract_pdf_text` returns an ordered page-scoped word accessibility projection with PDF-space boxes. The native viewer maps pointer drag rectangles to page coordinates, highlights selected words, and copies the selection through the existing Wayland clipboard on Ctrl+C. `AccessibleTextPage::select` remains available to other UI adapters. The viewer does not yet publish the words into an Orca AT-SPI tree. Office/media formats, full tagged-PDF semantics and cross-platform parser backends remain open parity cases.

## Word search (DROWZY-GUI-PARITY-004 D1)

`AccessibleTextPage::find(query, limit)` searches words already on the page.
It does not open the PDF, call a parser, or mutate the page. Trim the query;
an empty query returns no matches. Comparison uses Unicode scalar lowercase.
A match is one word or a run of adjacent words joined by a single space.
Each match records the inclusive start and end word indexes and the joined
text. `limit` is capped at 32. This is not Orca, ranking, or highlight
layout. Office, tagged PDF, and cross-platform parsers remain open.

## Text selection qualification spec

The same isolated worker may invoke `/usr/bin/pdftotext` with a single-page
range and `-bbox`, using the existing read-only input mount, resource limits,
no-network namespace, and wall deadline. The host returns a bounded page text
projection and word boxes in PDF page coordinates; the UI may select whole
words by rectangle intersection and copy the selected page text. This API is
page scoped and read only. HTML/XML entities must be decoded, malformed or
oversized output rejected, and text/word counts capped before returning data.
Accessibility exposure uses the same ordered word projection with page number
and bounds; this does not claim full tagged-PDF structure, reading order for
complex layouts, or Orca integration. Acceptance requires deterministic parser
bounds tests and real synthetic-PDF Poppler extraction under bwrap.
