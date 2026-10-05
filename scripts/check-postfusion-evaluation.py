#!/usr/bin/env python3
"""Lightweight checks of the predeclared acceptance arithmetic, no app/GPU."""
import importlib.util
from pathlib import Path
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


if __name__ == '__main__':
    unittest.main()
