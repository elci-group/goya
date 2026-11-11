# Goya measured HRTF profiles

Goya's spatial audio crate can load bounded directional FIR responses through
`MeasuredHrtfProfile::from_text`. The loader accepts the small `GY-HRTF/1`
text interchange format; it does not parse SOFA files. A host can convert a
lawfully obtained dataset to this format using a separately reviewed adapter.

```text
GY-HRTF/1
sample_rate_hz=48000
# azimuth_deg,elevation_deg,left_taps,right_taps
-30,0,0.8;0.1;-0.03,0.7;0.2;0.01
30,0,0.7;0.2;0.01,0.8;0.1;-0.03
```

The first line is mandatory. The sample rate must be a positive integer. Each
remaining data row has two finite direction values and two nonempty,
semicolon-separated tap lists. Azimuth is in `[-180, 180]` degrees, elevation
in `[-90, 90]`. Profiles are limited to 4,096 responses and 1,024 taps per
ear. Each tap's absolute value is limited to 16. Blank lines and trailing
`#` comments are ignored. Duplicate physical directions are rejected.

Lookup selects the measured direction with the greatest spherical-vector dot
product. Equal-distance ties retain the earlier row. It does not interpolate
between measurements. `sample_rate_hz()` reports the source rate; the caller
must supply a matching stream rate or resample the filters/audio. The FIR
renderer preserves input history across direction changes.

## Measured SOFA conversion

`tools/convert_sofa_hrtf.py` converts a locally obtained SOFA
`SimpleFreeFieldHRIR` file into `GY-HRTF/1`. It requires the separately
installed Python package `sofar` and supports spherical `SourcePosition`
coordinates in degrees, two receivers, and `Data.IR` shaped as
measurements × receivers × taps. It emits at most 4,096 unique directions and
defaults to the first 512 taps per ear (configurable up to the loader's 1,024
tap limit). The file is an interchange export, not an included or endorsed
profile. Check the source dataset's attribution and license before using or
redistributing either the source or converted data.

The Viking HRTF dataset v2 is a suitable measured source for evaluation. Its
record lists Simone Spagnol, Riccardo Miccini, and Rúnar Unnthórsson as
creators and declares CC BY 4.0. Cite [Zenodo record 4160401
(DOI 10.5281/zenodo.4160401)](https://zenodo.org/records/4160401) and link the
[CC BY 4.0 license](https://creativecommons.org/licenses/by/4.0/). The
[dataset documentation](https://sofacoustics.org/data/database/viking/documentation.pdf)
describes the measurements. Preserve these source details when distributing
derived exports.

This repository includes a 64-direction, 128-tap excerpt from `subj_Z.sofa`
at `crates/goya-spatial-audio/fixtures/viking-v2-subj-Z-first-64.gy-hrtf`.
The converter deterministically selects the first 64 measurement rows in SOFA
file order, skipping a later row when it is the same physical direction as an
earlier one under the loader's degree identity, and truncates each ear's response
to the first 128 taps. It does not resample or normalize coefficients. SOFA
azimuth is positive toward the listener's left, so the converter negates it
into Goya's right-positive azimuth, stores +180° as -180°, and writes
elevation ±90° with azimuth 0. Source
archive `HRTFs.zip` SHA-256:
`90971278359d667b962cc7f1d14ff5e8b8755349ebfc158245ec9b4a3a347fb0`;
source file `subj_Z.sofa` SHA-256:
`8206dffb8f40b141bdc16b34bce0800bd02ddb3f7fe4a9d68c3e7cc749cd633a`;
converted fixture SHA-256:
`dc590c59bffc461c4d7404fb06fde717e2bf572bd2f889950df5cc551deb37eb`.
The fixture is under the same CC BY 4.0 terms; its attribution is Simone
Spagnol, Riccardo Miccini, and Rúnar Unnthórsson, “The Viking HRTF dataset
v2,” Zenodo (2020), DOI 10.5281/zenodo.4160401, CC BY 4.0. The checked-in
excerpt is test/evaluation data, not a listener-specific recommendation.

Example, after obtaining a specific SOFA file and installing `sofar`:

```sh
python3 tools/convert_sofa_hrtf.py subject.sofa /tmp/viking-subject.gy-hrtf \
  --max-responses 4096 --max-taps 512
```

`sofar` 1.1.2 was used to produce the checked-in excerpt. The converter does
not download dataset files or write full source attribution into the compact
audio profile; keep provenance beside any separately produced export.

## Dataset status

The example at the top is synthetic and exists only to show the interchange
syntax. The included excerpt demonstrates loading a licensed measured profile;
full-dataset use still requires obtaining and tracking the original data under
its CC BY 4.0 attribution terms.
