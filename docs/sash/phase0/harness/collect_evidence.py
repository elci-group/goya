#!/usr/bin/env python3
"""
Sash Phase 0 Evidence Collector
Collects and packages all test evidence from Windows machines for Linux host analysis.
"""

import json
import os
import sys
import hashlib
import subprocess
import argparse
from pathlib import Path
from datetime import datetime
from typing import Dict, List, Any, Optional

class EvidenceCollector:
    def __init__(self, results_dir: str, output_dir: str):
        self.results_dir = Path(results_dir)
        self.output_dir = Path(output_dir)
        self.output_dir.mkdir(parents=True, exist_ok=True)

    def find_evidence_files(self) -> List[Path]:
        """Find all evidence JSON files in results directory."""
        patterns = [
            "*-evidence.json",
            "*-master_evidence.json",
            "*-import_closure.json",
            "*-a11y_snapshot.json",
            "*-dpi_evidence.json",
        ]
        files = []
        for pattern in patterns:
            files.extend(self.results_dir.glob(pattern))
        return sorted(files)

    def load_evidence(self, path: Path) -> Dict[str, Any]:
        """Load and validate evidence JSON."""
        try:
            with open(path, 'r', encoding='utf-8') as f:
                data = json.load(f)
            data['_source_file'] = str(path)
            data['_loaded_at'] = datetime.utcnow().isoformat() + 'Z'
            return data
        except Exception as e:
            return {
                '_source_file': str(path),
                '_error': f"Failed to load: {e}",
                '_loaded_at': datetime.utcnow().isoformat() + 'Z'
            }

    def compute_file_hash(self, path: Path) -> str:
        """Compute SHA-256 of a file."""
        sha256 = hashlib.sha256()
        with open(path, 'rb') as f:
            for chunk in iter(lambda: f.read(8192), b''):
                sha256.update(chunk)
        return sha256.hexdigest()

    def collect_binary_hashes(self, arch: str, profile: str) -> Dict[str, str]:
        """Compute hashes of the test binaries."""
        artifact_dir = Path(f"C:/sash-test/artifacts/{arch}/{profile}")
        binary_path = artifact_dir / "App.exe"
        if binary_path.exists():
            return {
                'exe_sha256': self.compute_file_hash(binary_path),
                'exe_size': binary_path.stat().st_size
            }
        return {}

    def package_evidence(self, arch: str, profile: str) -> Dict[str, Any]:
        """Package all evidence for a single arch/profile combination."""
        print(f"Packaging evidence for {arch}/{profile}...")

        # Find relevant evidence files
        all_files = self.find_evidence_files()
        relevant = [
            f for f in all_files
            if f.name.startswith(f"{arch}-{profile}-") or
               f.name.startswith(f"{arch}-{profile}_")
        ]

        evidence_bundle = {
            'arch': arch,
            'profile': profile,
            'packaged_at': datetime.utcnow().isoformat() + 'Z',
            'evidence_files': {},
            'binary_hashes': self.collect_binary_hashes(arch, profile),
            'summary': {
                'total_files': len(relevant),
                'checks': {},
                'overall': 'UNKNOWN'
            }
        }

        # Load each evidence file
        for ef in relevant:
            key = ef.stem.replace(f"{arch}-{profile}-", "").replace(f"{arch}-{profile}_", "")
            data = self.load_evidence(ef)
            evidence_bundle['evidence_files'][key] = data

            # Extract check results for summary
            if 'Checks' in data:
                evidence_bundle['summary']['checks'][key] = {
                    k: v.get('Pass', False) for k, v in data['Checks'].items()
                }
            elif 'Overall' in data:
                evidence_bundle['summary']['checks'][key] = {'Overall': data['Overall'] == 'PASS'}

        # Determine overall from master evidence if present
        master_key = 'master_evidence'
        if master_key in evidence_bundle['evidence_files']:
            master = evidence_bundle['evidence_files'][master_key]
            evidence_bundle['summary']['overall'] = master.get('Overall', 'UNKNOWN')
            evidence_bundle['summary']['test_summary'] = master.get('Summary', {})

        return evidence_bundle

    def create_phase0_report(self, all_bundles: List[Dict]) -> Dict[str, Any]:
        """Create consolidated Phase 0 report."""
        report = {
            'directive': 'SASH-ENT-001',
            'phase': 0,
            'generated_at': datetime.utcnow().isoformat() + 'Z',
            'architectures': {},
            'overall_status': 'PASS',
            'gates': {
                'static_window_x64': 'PENDING',
                'static_window_i686': 'PENDING',
                'static_window_arm64': 'PENDING',
                'interactive_x64': 'PENDING',
                'interactive_i686': 'PENDING',
                'interactive_arm64': 'PENDING',
            }
        }

        for bundle in all_bundles:
            arch = bundle['arch']
            profile = bundle['profile']
            key = f"{arch}_{profile}"

            report['architectures'][key] = {
                'arch': arch,
                'profile': profile,
                'overall': bundle['summary']['overall'],
                'binary': bundle['binary_hashes'],
                'checks': bundle['summary']['checks'],
            }

            # Update gates
            gate_key = f"{profile}_{arch}".replace('-', '_')
            if gate_key in report['gates']:
                report['gates'][gate_key] = bundle['summary']['overall']

            # Overall fails if any FAIL
            if bundle['summary']['overall'] == 'FAIL':
                report['overall_status'] = 'FAIL'

        return report

    def run(self, arches: List[str], profiles: List[str]) -> int:
        """Run collection for all arch/profile combinations."""
        print(f"Collecting evidence from {self.results_dir}")
        print(f"Output directory: {self.output_dir}")

        all_bundles = []
        for arch in arches:
            for profile in profiles:
                bundle = self.package_evidence(arch, profile)
                all_bundles.append(bundle)

                # Write individual bundle
                bundle_path = self.output_dir / f"{arch}_{profile}_evidence_bundle.json"
                with open(bundle_path, 'w', encoding='utf-8') as f:
                    json.dump(bundle, f, indent=2)
                print(f"  Wrote {bundle_path}")

        # Create consolidated report
        report = self.create_phase0_report(all_bundles)
        report_path = self.output_dir / "phase0_consolidated_report.json"
        with open(report_path, 'w', encoding='utf-8') as f:
            json.dump(report, f, indent=2)
        print(f"Wrote consolidated report: {report_path}")

        # Print summary
        print("\n=== Phase 0 Consolidated Report ===")
        print(f"Overall: {report['overall_status']}")
        print("\nGates:")
        for gate, status in report['gates'].items():
            print(f"  {gate}: {status}")

        print("\nArchitecture Details:")
        for key, arch_data in report['architectures'].items():
            print(f"  {key}: {arch_data['overall']}")
            if arch_data['binary']:
                print(f"    exe_sha256: {arch_data['binary']['exe_sha256'][:16]}...")
                print(f"    exe_size: {arch_data['binary']['exe_size']} bytes")

        return 0 if report['overall_status'] == 'PASS' else 1


def main():
    parser = argparse.ArgumentParser(description='Sash Phase 0 Evidence Collector')
    parser.add_argument('--results-dir', default='C:/sash-test/results',
                        help='Directory containing test results (on Windows)')
    parser.add_argument('--output-dir', default='./phase0_evidence',
                        help='Output directory for packaged evidence (on Linux)')
    parser.add_argument('--arches', nargs='+', default=['x64', 'i686', 'arm64'],
                        help='Architectures to collect')
    parser.add_argument('--profiles', nargs='+', default=['static-window', 'interactive-cut'],
                        help='Profiles to collect')

    args = parser.parse_args()

    collector = EvidenceCollector(args.results_dir, args.output_dir)
    return collector.run(args.arches, args.profiles)


if __name__ == '__main__':
    sys.exit(main())