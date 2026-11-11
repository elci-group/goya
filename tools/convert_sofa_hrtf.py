#!/usr/bin/env python3
"""Convert one SOFA SimpleFreeFieldHRIR file to bounded GY-HRTF/1 text.

Requires the separately installed `sofar` package. This tool does not fetch,
redistribute, or choose a dataset; the caller supplies a locally obtained file.
"""

import argparse
import math
from pathlib import Path

MAX_RESPONSES = 4096
MAX_TAPS = 1024
MAX_MAGNITUDE = 16.0


def scalar(value):
    """Extract a scalar from SOFA values represented as arrays or scalars."""
    try:
        return float(value.reshape(-1)[0])
    except AttributeError:
        return float(value)


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
    seen = set()
    count = min(int(ir.shape[0]), max_responses)
    if count < 1:
        raise SystemExit("SOFA file contains no measurements")
    for index in range(count):
        azimuth, elevation = float(positions[index, 0]), float(positions[index, 1])
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
        key = (round(azimuth, 5), round(elevation, 5))
        if key in seen:
            continue
        seen.add(key)
        ears = []
        for ear in range(2):
            taps = [float(v) for v in ir[index, ear, :max_taps]]
            if not taps or any(not math.isfinite(v) or abs(v) > MAX_MAGNITUDE for v in taps):
                raise SystemExit(f"invalid or out-of-bound impulse response in measurement {index}")
            ears.append(";".join(format(v, ".9g") for v in taps))
        rows.append(f"{azimuth:.5f},{elevation:.5f},{ears[0]},{ears[1]}")
    if len(seen) > MAX_RESPONSES:
        raise SystemExit("response limit exceeded")
    return "\n".join(rows) + "\n"


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
    main()
