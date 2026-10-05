#!/usr/bin/env python3
"""Lightweight checks of the predeclared acceptance arithmetic, no app/GPU."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('evaluation', Path(__file__).with_name('evaluate-postfusion.py'))
evaluation = importlib.util.module_from_spec(spec)
spec.loader.exec_module(evaluation)


class AcceptanceTests(unittest.TestCase):
    def rows(self, candidate=94):
        return {name: dict(p50=60, p95=candidate if fusion else 100, p99=120,
                           longFrameFraction=.1, inputSubmissionP95=40)
                for name, fusion in evaluation.ORDER}

    def test_clear_gain_passes_but_subthreshold_does_not(self):
        self.assertTrue(evaluation.decision(self.rows())['screenPass'])
        self.assertFalse(evaluation.decision(self.rows(96))['screenPass'])

    def test_noise_cannot_be_relabelled_as_gain(self):
        rows = self.rows()
        rows['AA2-A2']['p95'] = 108
        self.assertFalse(evaluation.decision(rows)['screenPass'])

    def test_one_tail_regression_cannot_hide_in_paired_median(self):
        rows = self.rows()
        rows['AB2-B']['p99'] = 122
        self.assertFalse(evaluation.decision(rows)['screenPass'])

    def test_one_negative_pair_is_not_stable(self):
        rows = self.rows()
        rows['AB2-B']['p95'] = 101
        self.assertFalse(evaluation.decision(rows)['screenPass'])

    def test_wrong_or_ineffective_variant_is_rejected_before_comparison(self):
        report = dict(valid=True, interruptions=[], runtime='Tauri / macOS WKWebView',
                      renderingOptimized=True, postFusionEnabled=True,
                      startingPostFusionEnabled=True, postFusionActive=False)
        with self.assertRaisesRegex(ValueError, 'ineffective'):
            evaluation.metrics(report, True, {})

    def test_invalid_run_is_not_a_slow_but_valid_result(self):
        with self.assertRaisesRegex(ValueError, 'Invalid/interrupted'):
            evaluation.metrics(dict(valid=False, interruptions=['hidden']), False, {})

    def inputs(self, offset=.375):
        return [dict(action=action, scheduledMs=ms + offset, timerDelayMs=800)
                for action, ms in evaluation.INPUT_SCHEDULE]

    def test_protocol_environment_is_hard_locked_at_manifest_entry(self):
        path = Path(__file__).parents[1] / 'docs/postfusion-night-manifest.example.json'
        template = json.loads(path.read_text())
        evaluation.validate_environment(template['environment'])
        for key in ['qaCommit', 'appSha256', 'powerCondition']:
            template[key] = 'test-only-declaration'
        template['visualReviewedPass'] = True
        for changes in [dict(viewport=[1920, 1182], devicePixelRatio=1),
                        dict(viewport=[1920, 1182]), dict(devicePixelRatio=1),
                        dict(canvas=[2880, 1773])]:
            with self.subTest(changes=changes), tempfile.TemporaryDirectory() as directory:
                manifest = json.loads(json.dumps(template))
                manifest['environment'].update(changes)
                target = Path(directory) / 'manifest.json'
                target.write_text(json.dumps(manifest))
                # Must fail before reading any reports, even if they might agree
                # with the altered manifest environment.
                with self.assertRaisesRegex(ValueError, 'Protocol environment changed'):
                    evaluation.evaluate(target)

    def test_exact_schedule_accepts_common_offset_and_late_delivery(self):
        self.assertEqual(len(evaluation.INPUT_SCHEDULE), 111)
        for offset in [0, .375, 5]:
            with self.subTest(offset=offset):
                evaluation.validate_input_schedule(self.inputs(offset))

    def test_action_reordering_is_rejected_despite_identical_counts(self):
        inputs = self.inputs()
        inputs[32]['action'], inputs[33]['action'] = inputs[33]['action'], inputs[32]['action']
        with self.assertRaisesRegex(ValueError, 'schedule order'):
            evaluation.validate_input_schedule(inputs)

    def test_wrong_planned_times_and_same_action_reordering_are_rejected(self):
        for change in ['all-zero', 'one-shift', 'swap-same-action', 'non-finite']:
            with self.subTest(change=change):
                inputs = self.inputs()
                if change == 'all-zero':
                    for item in inputs:
                        item['scheduledMs'] = 0
                elif change == 'one-shift':
                    inputs[80]['scheduledMs'] += 1
                elif change == 'swap-same-action':
                    inputs[0], inputs[1] = inputs[1], inputs[0]
                else:
                    inputs[80]['scheduledMs'] = float('nan')
                with self.assertRaises(ValueError):
                    evaluation.validate_input_schedule(inputs)

    def test_start_offset_is_bounded_independently_of_timer_delay(self):
        for offset in [-.1, 5.1, 125]:
            with self.subTest(offset=offset), self.assertRaisesRegex(ValueError, 'start offset'):
                evaluation.validate_input_schedule(self.inputs(offset))


if __name__ == '__main__':
    unittest.main()
