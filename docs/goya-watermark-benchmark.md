# Reproducible watermark robustness corpus

Run the default benchmark and capture its stable JSON report:

```sh
cargo run -p goya-bench --bin goya-watermark-benchmark -- \
  --seed 5141213315053340721 --images 12 --size 64 --key 0a11cec0ffee \
  --alpha 24 --output target/goya-watermark-report.json
```

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

No universal pass threshold is applied by the executable. Detection rates,
sample sizes, and distortions are reported as measurements. Existing unit
thresholds apply only to their exact fixture and parameters; they are
regression checks, not deployment claims. Use a separately licensed real-image
corpus and a real codec pipeline before publishing codec-specific thresholds.

Inputs are bounded to at most 512 images, 1024×1024 pixels each, and 8
million pixels total. Every run rejects invalid CLI values before allocating
the corpus.
