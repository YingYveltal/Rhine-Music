#!/usr/bin/env python3
"""Issue #18 night stress screen only; read saved reports, never run the app."""
import argparse
import hashlib
import json
import math
from pathlib import Path
from statistics import median

ORDER = [
    ('AA1-A1', False), ('AA1-A2', False), ('AB1-A', False), ('AB1-B', True),
    ('AA2-A1', False), ('AA2-A2', False), ('AB2-B', True), ('AB2-A', False),
    ('AA3-A1', False), ('AA3-A2', False), ('AB3-A', False), ('AB3-B', True),
]
QUALITY = dict(scale=100, pixelRatio=1.5, antialias='off', shadows=2048,
               aoSamples=32, aoResolution=1, depthOfField=100, transmission=1, anisotropy=16)


def require(condition, message):
    if not condition:
        raise ValueError(message)


def percentile(values, fraction):
    values = sorted(values)
    require(bool(values), 'Empty metric')
    require(all(isinstance(v, (int, float)) and math.isfinite(v) and v >= 0 for v in values),
            'Non-finite or negative metric')
    return values[min(len(values) - 1, int(len(values) * fraction))]


def metrics(report, variant, environment):
    require(report['valid'] and not report['interruptions'], 'Invalid/interrupted run')
    require(report['runtime'] == 'Tauri / macOS WKWebView', 'Wrong runtime')
    require(report['renderingOptimized'] is True, 'Rendering baseline changed')
    require(report['postFusionEnabled'] is variant and report['startingPostFusionEnabled'] is variant
            and report['postFusionActive'] is variant, 'Wrong or ineffective fusion variant')
    require(report['smoothMotion']['enabled'] is False and report['smoothMotion']['endingScale'] == 1
            and report['transmissionDepthEnabled'] is False, 'Other experiment active')
    require(not report['nativeMetal']['active'] and not report['nativeMetal']['preparing'], 'Metal active')
    require(report['benchmarkDevicePixelRatio'] is None, 'DPR override active')
    for name in ['canvas', 'viewport', 'devicePixelRatio', 'userAgent']:
        require(report[name] == environment[name], 'Environment mismatch: ' + name)
    require(report['startingCanvas'] == environment['canvas']
            and report['startingDevicePixelRatio'] == environment['devicePixelRatio'], 'Display changed')
    meta = report['metadata']
    require(meta['quality'] == QUALITY and meta['theme'] == 'night' and meta['albums'] == 16
            and meta['rateHz'] == 8 and meta['sortMode'] == 'album', 'Wrong quality, theme, fixture size or replay')
    require(meta['startingAlbum'] == environment['expectedFinalAlbum'] and meta['startingPhase'] == 'archive',
            'Wrong initial album/phase')
    interaction = report['interactions']
    inputs = interaction['raw']
    require(interaction['count'] == 111 and len(inputs) == 111, 'Incomplete replay')
    require(interaction['finalState'] == dict(albumId=environment['expectedFinalAlbum'],
            pending=False, phase='detail') and meta['expectedFinalAlbum'] == environment['expectedFinalAlbum'],
            'Incorrect final album or phase')
    expected_actions = {'album-burst': 32, 'detail-switch-burst': 40, 'genre-burst': 32,
                        'open-interrupt': 2, 'back-interrupt': 2, 'open': 1, 'back': 1, 'final-album-open': 1}
    counts = {name: sum(i['action'] == name for i in inputs) for name in expected_actions}
    require(counts == expected_actions, 'Unexpected input schedule')
    columns = [i['pose'][10] for i in inputs if i['action'] == 'genre-burst']
    require(max(columns) - min(columns) > .001, 'No actual cross-column motion')
    raw = report['raw']
    require(len(raw) == report['samples'] and bool(raw), 'Missing raw frames')
    require(all([f['width'], f['height']] == environment['canvas'] for f in raw), 'Buffer changed')
    intervals = [f['interval'] for f in raw]
    require(report['coverage'] >= .9 and 28000 <= report['elapsedMs'] < 30000, 'Incomplete duration')
    p95 = percentile(intervals, .95)
    require(all(value > 0 for value in intervals) and abs(sum(intervals) / 28000 - report['coverage']) < 1e-6,
            'Coverage/raw mismatch')
    require(abs(p95 - report['frameMs']['p95']) < 1e-6, 'Summary/raw mismatch')
    return dict(p50=percentile(intervals, .5), p95=p95, p99=percentile(intervals, .99),
                longFrameFraction=sum(v > 50 for v in intervals) / len(intervals),
                inputSubmissionP95=percentile([i['nextSubmissionMs'] for i in inputs], .95))


def decision(rows):
    aa = [(rows[f'AA{i}-A1'], rows[f'AA{i}-A2']) for i in range(1, 4)]
    ab = [(rows[f'AB{i}-A'], rows[f'AB{i}-B']) for i in range(1, 4)]
    require(all(a['p95'] > 0 for a, _ in aa + ab), 'Zero baseline P95')
    noise = max(abs(b['p95'] - a['p95']) / a['p95'] for a, b in aa)
    gains = [(a['p95'] - b['p95']) / a['p95'] for a, b in ab]
    tolerances = {key: max(abs(b[key] - a[key]) for a, b in aa)
                  for key in ['p99', 'longFrameFraction', 'inputSubmissionP95']}
    regressions = [{key: b[key] - a[key] for key in tolerances} for a, b in ab]
    passed = (median(gains) >= .05 and median(gains) > noise and min(gains) > 0
              and all(row[key] <= tolerance + 1e-9
                      for row in regressions for key, tolerance in tolerances.items()))
    return dict(screenPass=passed, medianP95Improvement=median(gains), pairedP95Improvements=gains,
                aaP95Noise=noise, aaAbsoluteTolerances=tolerances, pairedRegressions=regressions,
                next='Expand only after coordinator review' if passed else 'Keep fusion OFF; stop candidate')


def evaluate(manifest_path):
    manifest_path = Path(manifest_path)
    manifest = json.loads(manifest_path.read_text())
    require(manifest['protocol'] == 'issue18-night-v1', 'Unknown protocol')
    require(manifest['visualReviewedPass'] is True, 'Visual review must pass before performance')
    for key in ['baselineCommit', 'qaCommit', 'appSha256', 'fixtureManifestSha256', 'storeId', 'powerCondition']:
        require(bool(manifest[key]), 'Missing frozen binding: ' + key)
    require(manifest['environment']['canvas'] == [1920, 1182], 'Protocol buffer changed')
    require([r['id'] for r in manifest['runs']] == [name for name, _ in ORDER], 'Run order changed')
    rows, files, timestamps = {}, [], []
    for run, (name, variant) in zip(manifest['runs'], ORDER):
        require(not run['externalInterruption'], name + ': external interruption; no selection of good runs')
        path = manifest_path.parent / run['file']
        data = path.read_bytes()
        digest = hashlib.sha256(data).hexdigest()
        require(digest == run['sha256'], name + ': evidence hash mismatch')
        report = json.loads(data)
        rows[name] = metrics(report, variant, manifest['environment'])
        timestamps.append(report['measuredAt'])
        files.append(dict(id=name, path=run['file'], sha256=digest))
    require(len(set(timestamps)) == len(timestamps) and timestamps == sorted(timestamps), 'Reports reused/reordered')
    return dict(protocol=manifest['protocol'], bindings={k: manifest[k] for k in
                ['baselineCommit', 'qaCommit', 'appSha256', 'fixtureManifestSha256', 'storeId', 'powerCondition']},
                environment=manifest['environment'], files=files, metrics=rows, decision=decision(rows),
                note='RAF and event-to-submission only; not GPU completion or real display latency.')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('manifest', type=Path)
    args = parser.parse_args()
    try:
        print(json.dumps(evaluate(args.manifest), indent=2))
    except (ValueError, KeyError, TypeError, OSError) as error:
        raise SystemExit('STOP: no valid comparison: ' + str(error))
