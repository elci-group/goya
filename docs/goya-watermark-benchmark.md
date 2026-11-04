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

cargo run -p goya-bench --bin goya-watermark-benchmark -- \
  --images 6 --size 64 --codec h264 --codec mpeg4
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

The optional section reports the ffmpeg version, quality parameter and
settings, exact-payload hits, sample counts, detection rates, and mean maximum
pixel deltas. Multiple `--codec` selections are kept as separate entries in
`codec_roundtrips`; `--codec-roundtrip` alone remains an MJPEG shorthand. The
available settings are MJPEG `-q:v` 2/5/8, PNG `-compression_level` 1/6/9,
H.264/libx264 `-crf` 18/28/38 in Matroska with `yuv420p`, and MPEG-4 Part 2
`-q:v` 2/5/8 in AVI with `yuv420p`. If ffmpeg is missing or a selected codec
step fails, that codec entry is `unavailable` with a reason; remaining codecs
and deterministic pixel-surrogate results are still reported. At most four
codec measurements can be selected per run. No ffmpeg crate or binary is
bundled or linked by this package. The installed ffmpeg build and its enabled
codecs are external system configuration.

An expanded repeatability sample measured all four codec paths in one run:

```sh
cargo run -p goya-bench --bin goya-watermark-benchmark -- \
  --seed 5141213315053340721 --images 6 --size 64 \
  --key 0a11cec0ffee --alpha 24 \
  --codec mjpeg --codec png --codec h264 --codec mpeg4 \
  --output target/goya-codec-sweep.json
```

The corpus uses six deterministic patterns at `64×64`; payloads and transforms
are seed-derived. Both identical invocations produced byte-identical JSON
(SHA-256 `b5aa58b095f58a397dcd4f2cefc51a016eb71852660e925d81cdefbbfd058cde`).
The encoder build was `ffmpeg version 6.1.1-3ubuntu5`; its `libavutil` was
58.29.100, `libavcodec` 60.31.102, and `libavformat` 60.16.100. SHA-256 of the
complete `ffmpeg -version` output, including build configuration, was
`5b320c97f515e79171f10a2147d875ffb6b14bd2b25681047b73c46fe0a527ee`.

| Codec | Settings | Exact payloads | Mean max pixel delta |
|---|---:|---:|---:|
| MJPEG | `-q:v` 2 | 6/6 | 5.500 |
| MJPEG | `-q:v` 5 | 6/6 | 12.833 |
| MJPEG | `-q:v` 8 | 6/6 | 15.500 |
| PNG | `-compression_level` 1 | 6/6 | 0.000 |
| PNG | `-compression_level` 6 | 6/6 | 0.000 |
| PNG | `-compression_level` 9 | 6/6 | 0.000 |
| H.264/libx264, Matroska, `yuv420p`, preset `medium` | CRF 18 | 6/6 | 13.500 |
| H.264/libx264, Matroska, `yuv420p`, preset `medium` | CRF 28 | 1/6 | 36.333 |
| H.264/libx264, Matroska, `yuv420p`, preset `medium` | CRF 38 | 0/6 | 91.000 |
| MPEG-4 Part 2, AVI, `yuv420p` | `-q:v` 2 | 6/6 | 4.500 |
| MPEG-4 Part 2, AVI, `yuv420p` | `-q:v` 5 | 6/6 | 9.167 |
| MPEG-4 Part 2, AVI, `yuv420p` | `-q:v` 8 | 4/6 | 15.667 |

These are measurements for this exact synthetic corpus, selected encoder
settings, and ffmpeg build only. They do not imply performance on photographs,
other codec versions, other image normalization paths, or deployment data.

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

The optional MJPEG, PNG, H.264, and MPEG-4 measurements are real codec
encode/decode round-trips, but they cover only the selected synthetic
grayscale corpus, this ffmpeg build, and the reported settings. They are not
evidence for other codec implementations, color images, photographs, or
deployment robustness. PNG round-trips are lossless codec controls; they do
not measure compression damage. Pixel deltas compare decoded output against
the watermarked input, and detection requires exact payload recovery.

No universal pass threshold is applied by the executable. Detection rates,
sample sizes, and distortions are reported as measurements. Existing unit
thresholds apply only to their exact fixture and parameters; they are
regression checks, not deployment claims. Use a separately licensed real-image
corpus and real codec pipelines before publishing broader codec-specific
thresholds.

Inputs are bounded to at most 512 images, 1024×1024 pixels each, and 8
million pixels total. Every run rejects invalid CLI values before allocating
the corpus.
