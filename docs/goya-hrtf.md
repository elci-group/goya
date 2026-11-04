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

The Viking HRTF dataset v2 is a suitable measured source for evaluation: it
contains full-sphere measurements and is published by Simone Spagnol, Kristján
Bjarki Purkhús, Sverrir Karl Björnsson, and Rúnar Unnthórsson under CC BY 4.0.
The dataset record is [Zenodo 4160401](https://zenodo.org/records/4160401);
its [dataset documentation](https://sofacoustics.org/data/database/viking/documentation.pdf)
describes the measurements. Retain attribution to those authors and link the
CC BY 4.0 license when distributing derived exports.

Example, after obtaining a specific SOFA file and installing `sofar`:

```sh
python3 tools/convert_sofa_hrtf.py subject.sofa /tmp/viking-subject.gy-hrtf \
  --max-responses 4096 --max-taps 512
```

The remaining external gate is to select and obtain a particular SOFA file,
record its exact filename/subject and checksum, and validate the converter
against that file. No measured data is checked into this repository. The
converter deliberately does not download the dataset or write source
attribution into the compact audio profile; keep source provenance beside the
export.

## Dataset status

No measured HRTF data is bundled in this checkout. The example at the top is
synthetic and exists only to show the interchange syntax. The converter and
the identified CC BY 4.0 dataset now provide a documented path to an
attributed measured profile once an integrator obtains a source file.
