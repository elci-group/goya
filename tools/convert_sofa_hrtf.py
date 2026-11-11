#!/usr/bin/env python3
"""Convert one SOFA SimpleFreeFieldHRIR file to bounded GY-HRTF/1 text.

Requires the separately installed `sofar` package. This tool does not fetch,
redistribute, or choose a dataset; the caller supplies a locally obtained file.
"""

import argparse
import math
import sys
from pathlib import Path

MAX_RESPONSES = 4096
MAX_TAPS = 1024
MAX_MAGNITUDE = 16.0
# Same physical-direction identity as MeasuredHrtfProfile: exact poles, otherwise
# circular azimuth and elevation deltas strictly under 1e-3 degrees.
DIRECTION_EPSILON_DEG = 1e-3


def scalar(value):
    """Extract a scalar from SOFA values represented as arrays or scalars."""
    try:
        return float(value.reshape(-1)[0])
    except AttributeError:
        return float(value)


def normalize_sofa_direction(azimuth: float, elevation: float, index: int) -> tuple[float, float]:
    """Map one SOFA spherical position into Goya's right-positive degrees."""
    if not math.isfinite(azimuth) or not math.isfinite(elevation):
        raise SystemExit(f"invalid direction in measurement {index}")
    # SOFA spherical azimuth is positive toward the listener's left.
    # Goya azimuth is positive toward the listener's right.
    azimuth = -azimuth
    azimuth = ((azimuth + 180.0) % 360.0) - 180.0
    if abs(azimuth - 180.0) < 1e-9:
        azimuth = -180.0
    if abs(elevation) > 90.0:
        raise SystemExit(f"elevation out of range in measurement {index}")
    if abs(abs(elevation) - 90.0) < 1e-9:
        azimuth = 0.0
        elevation = math.copysign(90.0, elevation)
    return azimuth, elevation


def same_measured_direction(left_az: float, left_el: float, right_az: float, right_el: float) -> bool:
    """Poles share one direction per sign; other rows use degree identity."""
    if (
        abs(left_el) == 90.0
        and abs(right_el) == 90.0
        and math.copysign(1.0, left_el) == math.copysign(1.0, right_el)
    ):
        return True
    azimuth_delta = abs(left_az - right_az) % 360.0
    azimuth_delta = min(azimuth_delta, 360.0 - azimuth_delta)
    return (
        azimuth_delta < DIRECTION_EPSILON_DEG
        and abs(left_el - right_el) < DIRECTION_EPSILON_DEG
    )


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
        rows.append(f"{azimuth:.5f},{elevation:.5f},{ears[0]},{ears[1]}")
    if len(kept) > MAX_RESPONSES:
        raise SystemExit("response limit exceeded")
    return "\n".join(rows) + "\n"


def check_direction_identity() -> None:
    """Degree identity, not five-decimal rounding, decides duplicate rows."""
    kept: list[tuple[float, float]] = []
    assert keep_unique_direction(kept, 0.0, 0.0)
    # 4e-4 degrees is under 1e-3 but survives round(..., 5).
    assert not keep_unique_direction(kept, 0.0004, 0.0)
    assert keep_unique_direction(kept, 0.001, 0.0)
    assert keep_unique_direction(kept, 0.0, 89.0)
    assert keep_unique_direction(kept, 1.0, 89.0)

    poles: list[tuple[float, float]] = []
    assert keep_unique_direction(poles, 0.0, 90.0)
    assert not keep_unique_direction(poles, 45.0, 90.0)
    assert keep_unique_direction(poles, 0.0, -90.0)
    assert not keep_unique_direction(poles, 170.0, -90.0)

    seam: list[tuple[float, float]] = []
    assert keep_unique_direction(seam, 180.0, 0.0)
    assert not keep_unique_direction(seam, -180.0, 0.0)
    assert keep_unique_direction(seam, 179.9996, 10.0)
    assert not keep_unique_direction(seam, -179.9996, 10.0)

    assert normalize_sofa_direction(90.0, 0.0, 0) == (-90.0, 0.0)
    assert normalize_sofa_direction(270.0, 0.0, 1) == (90.0, 0.0)
    assert normalize_sofa_direction(180.0, -45.0, 2) == (-180.0, -45.0)
    assert normalize_sofa_direction(30.0, 90.0, 3) == (0.0, 90.0)
    assert normalize_sofa_direction(30.0, -90.0, 4) == (0.0, -90.0)


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
