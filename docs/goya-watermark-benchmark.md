# Reproducible watermark robustness corpus

Run the default benchmark and capture its stable JSON report:

```sh
cargo run -p goya-bench --bin goya-watermark-benchmark -- \
  --seed 5141213315053340721 --images 12 --size 64 --key 0a11cec0ffee \
  --alpha 24 --output target/goya-watermark-report.json
```

By default `codec_roundtrip.status` is `not_requested`. To encode each marked
grayscale image and decode it through the locally installed `ffmpeg`, pass
`--codec-roundtrip` (MJPEG) or select PNG explicitly:

```sh
cargo run -p goya-bench --bin goya-watermark-benchmark -- \
  --images 6 --size 64 --codec-roundtrip

cargo run -p goya-bench --bin goya-watermark-benchmark -- \
  --images 6 --size 64 --codec png
```

To evaluate a caller-selected local corpus, put supported image files directly
in a directory and pass `--image-dir`. The benchmark sorts paths
lexicographically, probes source dimensions, and uses ffmpeg to take the first
video frame, fit it into the requested square size while preserving aspect
ratio, pad with neutral gray, and convert to 8-bit grayscale. Both the
deterministic transform suite and optional MJPEG/PNG codec round-trips use
those same normalized source pixels:

```sh
cargo run -p goya-bench --bin goya-watermark-benchmark -- \
  --image-dir ./licensed-images --size 256 --codec mjpeg \
  --output target/goya-local-image-report.json
```

The report records each input path, SHA-256 of the original file bytes, source
dimensions, normalized dimensions, and the ffmpeg version used for decoding.
Local inputs are limited to 512 files, 64 MiB per file, 256 MiB combined,
8192 pixels per source edge, 8 million source pixels total, and 8 million
normalized pixels total. Source dimensions are checked with ffprobe before
ffmpeg decodes image pixels; decoded output is capped at the exact normalized
frame size. The CLI reads supported image files in the named directory (not
recursively); the library API accepts explicit paths and applies the same
deterministic ordering and bounds.

Callers are responsible for selecting and having rights to use their local
datasets. Goya does not bundle or redistribute image data. A measurement only
describes the listed files, normalization, watermark parameters, ffmpeg build,
and transformations in that report; it does not establish general robustness
or rights to publish source imagery. Check licensing and redact source paths
before sharing reports where paths are sensitive.

The optional section reports the ffmpeg version, three MJPEG encoder quality
settings (`-q:v` 2, 5, and 8) or three PNG compression settings
(`-compression_level` 1, 6, and 9), exact-payload hits, sample counts,
detection rates, and mean maximum pixel deltas. Run separately for each codec;
the report has one codec field. If ffmpeg is missing or a codec step fails,
`status` is `unavailable` with a reason and no codec samples; the deterministic
pixel-surrogate results are still produced. No ffmpeg crate or binary is
bundled or linked by this package. The installed ffmpeg build and its enabled
codecs are external system configuration.

The executable records the seed, image count and dimensions, key, embedding
strength, corpus fingerprint, per-transform sample and hit counts, mean pixel
delta, and unmarked-image false-positive controls. Repeating the same command
produces the same report bytes. The FNV-1a corpus fingerprint is an
identification checksum, not a security digest.

The generated corpus is synthetic and deterministic. It cycles through six
classes: smooth gradients, checkerboards, pseudorandom texture, narrow
stripes, radial rings, and low-contrast checks. Each image has a distinct
payload. The false-positive control uses the corresponding unmarked image
with the requested key plus sixteen stable derived keys.

The standard attack suite measures block quantisation (`JPEGish`), nearest
neighbour resampling, centered crops, bounded pixel noise, integer contrast
and brightness edits, box blur, and salt-and-pepper noise. These operations
are deterministic pixel-domain surrogates. `JPEGish` is not a JPEG encoder or
decoder round-trip, and this corpus does not establish robustness on
photographs, arbitrary codecs, or attacks absent from the report.

The optional MJPEG and PNG measurements are real codec encode/decode
round-trips, but they cover only this synthetic grayscale corpus, this ffmpeg
build, and the reported settings. They are not evidence for other codec
implementations, color images, photographs, or deployment robustness. PNG
round-trips are lossless codec controls; they do not measure compression
damage. Pixel deltas compare decoded output against the watermarked input, and
detection requires exact payload recovery.

No universal pass threshold is applied by the executable. Detection rates,
sample sizes, and distortions are reported as measurements. Existing unit
thresholds apply only to their exact fixture and parameters; they are
regression checks, not deployment claims. Use a separately licensed real-image
corpus and real codec pipelines before publishing broader codec-specific
thresholds.

Inputs are bounded to at most 512 images, 1024×1024 pixels each, and 8
million pixels total. Every run rejects invalid CLI values before allocating
the corpus.
