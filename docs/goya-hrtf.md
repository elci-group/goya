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

## Dataset status

No measured HRTF dataset was found in this checkout or in the searched local
home and system share directories. The example above is synthetic and exists
only to show the interchange syntax. Goya does not bundle measured data;
integrators must obtain a dataset with suitable usage rights and retain its
attribution and license outside the core crate.
