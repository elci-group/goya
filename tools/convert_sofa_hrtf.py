#!/usr/bin/env python3
"""Convert one SOFA SimpleFreeFieldHRIR file to bounded GY-HRTF/1 text.

Requires the separately installed `sofar` package. This tool does not fetch,
redistribute, or choose a dataset; the caller supplies a locally obtained file.
"""

import argparse
import math
import struct
import sys
from pathlib import Path

MAX_RESPONSES = 4096
MAX_TAPS = 1024
MAX_MAGNITUDE = 16.0
# Loader identity on the five-decimal text, parsed and compared as f32.
# Poles at ±90 share one direction per sign; otherwise circular azimuth and
# elevation deltas are strictly under 1e-3 degrees.
DIRECTION_EPSILON_DEG = 1e-3


def scalar(value):
    """Extract a scalar from SOFA values represented as arrays or scalars."""
    try:
        return float(value.reshape(-1)[0])
    except AttributeError:
        return float(value)


def f32(value: float) -> float:
    """Round one degree value to the f32 the loader parses and subtracts."""
    return struct.unpack("<f", struct.pack("<f", float(value)))[0]


def canonical_direction_text(azimuth: float, elevation: float) -> tuple[str, str]:
    """Text the writer emits. +180 is stored as -180; ±90 uses azimuth 0."""
    azimuth_text = f"{azimuth:.5f}"
    elevation_text = f"{elevation:.5f}"
    if elevation_text in ("90.00000", "-90.00000"):
        azimuth_text = "0.00000"
    elif azimuth_text == "180.00000":
        azimuth_text = "-180.00000"
    return azimuth_text, elevation_text


def normalize_sofa_direction(azimuth: float, elevation: float, index: int) -> tuple[float, float]:
    """Map one SOFA spherical position into Goya's right-positive degrees."""
    if not math.isfinite(azimuth) or not math.isfinite(elevation):
        raise SystemExit(f"invalid direction in measurement {index}")
    # SOFA spherical azimuth is positive toward the listener's left.
    # Goya azimuth is positive toward the listener's right.
    azimuth = -azimuth
    azimuth = ((azimuth + 180.0) % 360.0) - 180.0
    azimuth_text, elevation_text = canonical_direction_text(azimuth, elevation)
    azimuth_f = f32(float(azimuth_text))
    elevation_f = f32(float(elevation_text))
    if (
        not math.isfinite(elevation_f)
        or not (-90.0 <= elevation_f <= 90.0)
    ):
        raise SystemExit(f"elevation out of range in measurement {index}")
    if not math.isfinite(azimuth_f) or not (-180.0 <= azimuth_f <= 180.0):
        raise SystemExit(f"invalid direction in measurement {index}")
    return float(azimuth_text), float(elevation_text)


def same_measured_direction(left_az: float, left_el: float, right_az: float, right_el: float) -> bool:
    """Poles share one direction per sign; other rows use the loader's f32 identity."""
    left_az_text, left_el_text = canonical_direction_text(left_az, left_el)
    right_az_text, right_el_text = canonical_direction_text(right_az, right_el)
    left_az_f = f32(float(left_az_text))
    left_el_f = f32(float(left_el_text))
    right_az_f = f32(float(right_az_text))
    right_el_f = f32(float(right_el_text))
    if (
        abs(left_el_f) == 90.0
        and abs(right_el_f) == 90.0
        and math.copysign(1.0, left_el_f) == math.copysign(1.0, right_el_f)
    ):
        return True
    # f32 subtraction, then Rust `%` (truncating remainder) and the short arc.
    azimuth_delta = abs(f32(left_az_f - right_az_f))
    if azimuth_delta >= 360.0:
        turns = math.trunc(f32(azimuth_delta / f32(360.0)))
        azimuth_delta = abs(f32(azimuth_delta - f32(turns) * f32(360.0)))
    other_arc = f32(f32(360.0) - azimuth_delta)
    if other_arc < azimuth_delta:
        azimuth_delta = other_arc
    elevation_delta = abs(f32(left_el_f - right_el_f))
    limit = f32(DIRECTION_EPSILON_DEG)
    return azimuth_delta < limit and elevation_delta < limit


def keep_unique_direction(kept: list[tuple[float, float]], azimuth: float, elevation: float) -> bool:
    """Record a direction unless an earlier row is the same physical direction."""
    if any(same_measured_direction(azimuth, elevation, prev_az, prev_el) for prev_az, prev_el in kept):
        return False
    kept.append((azimuth, elevation))
    return True


def convert(source: Path, max_responses: int, max_taps: int) -> str:
    try:
        import sofar
    except ImportError as error:
        raise SystemExit("Missing dependency: install `sofar` in your environment") from error

    sofa = sofar.read_sofa(str(source))
    if getattr(sofa, "GLOBAL_SOFAConventions", "") != "SimpleFreeFieldHRIR":
        raise SystemExit("Only SOFA SimpleFreeFieldHRIR files are supported")
    ir = sofa.Data_IR
    positions = sofa.SourcePosition
    if len(ir.shape) != 3 or ir.shape[1] != 2 or len(positions.shape) != 2 or positions.shape[1] < 2:
        raise SystemExit("Expected Data.IR [measurements, 2 receivers, taps] and SourcePosition [measurements, 3]")
    rate = int(round(scalar(sofa.Data_SamplingRate)))
    if rate <= 0:
        raise SystemExit("invalid SOFA sampling rate")
    position_type = getattr(sofa, "SourcePosition_Type", "spherical")
    position_units = getattr(sofa, "SourcePosition_Units", "degree, degree, metre")
    if str(position_type).lower() != "spherical" or "degree" not in str(position_units).lower():
        raise SystemExit("Only spherical SourcePosition values in degrees are supported")

    rows = ["GY-HRTF/1", f"sample_rate_hz={rate}", "# Converted from caller-supplied SOFA SimpleFreeFieldHRIR; see provenance supplied by caller."]
    kept: list[tuple[float, float]] = []
    count = min(int(ir.shape[0]), max_responses)
    if count < 1:
        raise SystemExit("SOFA file contains no measurements")
    for index in range(count):
        azimuth, elevation = normalize_sofa_direction(
            float(positions[index, 0]), float(positions[index, 1]), index
        )
        if not keep_unique_direction(kept, azimuth, elevation):
            continue
        ears = []
        for ear in range(2):
            taps = [float(v) for v in ir[index, ear, :max_taps]]
            if not taps or any(not math.isfinite(v) or abs(v) > MAX_MAGNITUDE for v in taps):
                raise SystemExit(f"invalid or out-of-bound impulse response in measurement {index}")
            ears.append(";".join(format(v, ".9g") for v in taps))
        azimuth_text, elevation_text = canonical_direction_text(azimuth, elevation)
        rows.append(f"{azimuth_text},{elevation_text},{ears[0]},{ears[1]}")
    if len(kept) > MAX_RESPONSES:
        raise SystemExit("response limit exceeded")
    return "\n".join(rows) + "\n"


def check_direction_identity() -> None:
    """Loader f32 identity of the written text decides duplicate rows."""
    kept: list[tuple[float, float]] = []
    assert keep_unique_direction(kept, 0.0, 0.0)
    # 4e-4 degrees is under 1e-3 but survives round(..., 5).
    assert not keep_unique_direction(kept, 0.0004, 0.0)
    assert keep_unique_direction(kept, 0.001, 0.0)
    assert keep_unique_direction(kept, 0.0, 89.0)
    assert keep_unique_direction(kept, 1.0, 89.0)
    # Decimal gap is exactly 0.001, but f32 subtraction rounds under 1e-3.
    azimuth_step = []
    assert keep_unique_direction(azimuth_step, 0.00050, 0.0)
    assert not keep_unique_direction(azimuth_step, 0.00150, 0.0)
    elevation_step = []
    assert keep_unique_direction(elevation_step, 0.0, 0.00050)
    assert not keep_unique_direction(elevation_step, 0.0, 0.00150)
    # The same printed gap rounds the other way and stays distinct.
    distinct = []
    assert keep_unique_direction(distinct, 0.01000, 0.0)
    assert keep_unique_direction(distinct, 0.01100, 0.0)
    rounded = []
    assert keep_unique_direction(rounded, 0.0, 0.0)
    assert keep_unique_direction(rounded, 0.000995, 0.0)

    poles: list[tuple[float, float]] = []
    assert keep_unique_direction(poles, 0.0, 90.0)
    assert not keep_unique_direction(poles, 45.0, 90.0)
    assert keep_unique_direction(poles, 0.0, -90.0)
    assert not keep_unique_direction(poles, 170.0, -90.0)
    # Values that print as ±90 are one pole, including a non-zero azimuth.
    near_poles: list[tuple[float, float]] = []
    assert keep_unique_direction(near_poles, 12.0, 89.999996)
    assert not keep_unique_direction(near_poles, 40.0, 89.999997)
    assert not keep_unique_direction(near_poles, 10.0, 90.0)
    assert keep_unique_direction(near_poles, 12.0, -89.999996)
    assert not keep_unique_direction(near_poles, -20.0, -90.0)
    # 89.99999 does not print as 90, so a different azimuth stays distinct.
    assert keep_unique_direction(near_poles, 45.0, 89.99999)

    seam: list[tuple[float, float]] = []
    assert keep_unique_direction(seam, 180.0, 0.0)
    assert not keep_unique_direction(seam, -180.0, 0.0)
    assert keep_unique_direction(seam, 179.9996, 10.0)
    assert not keep_unique_direction(seam, -179.9996, 10.0)
    across = []
    assert keep_unique_direction(across, 179.99950, 0.0)
    assert keep_unique_direction(across, -179.99950, 0.0)

    assert normalize_sofa_direction(90.0, 0.0, 0) == (-90.0, 0.0)
    assert normalize_sofa_direction(270.0, 0.0, 1) == (90.0, 0.0)
    assert normalize_sofa_direction(180.0, -45.0, 2) == (-180.0, -45.0)
    assert normalize_sofa_direction(30.0, 90.0, 3) == (0.0, 90.0)
    assert normalize_sofa_direction(30.0, -90.0, 4) == (0.0, -90.0)
    assert normalize_sofa_direction(30.0, 89.999996, 5) == (0.0, 90.0)
    assert normalize_sofa_direction(30.0, -89.999996, 6) == (0.0, -90.0)
    assert normalize_sofa_direction(-179.999996, 0.0, 7) == (-180.0, 0.0)
    assert normalize_sofa_direction(15.0, 90.000004, 8) == (0.0, 90.0)
    written = normalize_sofa_direction(30.0, 89.999996, 5)
    assert f"{written[0]:.5f},{written[1]:.5f}" == "0.00000,90.00000"
    written = normalize_sofa_direction(-179.999996, 12.0, 7)
    assert f"{written[0]:.5f},{written[1]:.5f}" == "-180.00000,12.00000"
    for azimuth, elevation in ((0.0, -45.0), (5.0, -45.0), (-180.0, -45.0), (90.0, -45.0)):
        assert canonical_direction_text(azimuth, elevation) == (
            f"{azimuth:.5f}",
            f"{elevation:.5f}",
        )
    try:
        normalize_sofa_direction(0.0, 90.000006, 9)
    except SystemExit:
        pass
    else:
        raise AssertionError("elevation past ±90 must be rejected")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("sofa_file", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--max-responses", type=int, default=4096)
    parser.add_argument("--max-taps", type=int, default=512)
    args = parser.parse_args()
    if not 1 <= args.max_responses <= MAX_RESPONSES or not 1 <= args.max_taps <= MAX_TAPS:
        parser.error("max-responses must be 1..4096 and max-taps 1..1024")
    args.output.write_text(convert(args.sofa_file, args.max_responses, args.max_taps), encoding="utf-8")


if __name__ == "__main__":
    if len(sys.argv) == 2 and sys.argv[1] == "--check-direction-identity":
        check_direction_identity()
    else:
        main()
