#!/usr/bin/env python3
"""Additional ranking checks; all pre-existing self-tests remain unchanged."""
import collections
import unittest
from rank_blockers import candidates


class CandidateTests(unittest.TestCase):
    def test_four_fixes_combine_two_complete_requirement_sets(self):
        buckets = collections.Counter({frozenset(['a', 'b']): 2,
                                       frozenset(['c', 'd']): 3})
        rows = candidates(buckets, 15)
        item = next(row for row in rows if set(row['fixes']) == {'a', 'b', 'c', 'd'})
        self.assertEqual(item['predicted_cfg_gain'], 5)
        self.assertEqual(item['validation_minutes'], 15)

    def test_complete_larger_bundle_is_visible_without_partial_gain(self):
        required = frozenset(f'op{i}' for i in range(6))
        rows = candidates(collections.Counter({required: 5}), 15)
        item = next(row for row in rows if set(row['fixes']) == required)
        self.assertEqual(item['predicted_cfg_gain'], 5)
        self.assertTrue(all(row['predicted_cfg_gain'] == 0 for row in rows
                            if set(row['fixes']) != required))

    def test_runtime_requirement_cannot_be_scored_as_cheap_isa(self):
        rows = candidates(collections.Counter({frozenset(['a', 'runtime:svc']): 99,
                                               frozenset(['a']): 2}), 15)
        self.assertTrue(all('runtime:svc' not in row['fixes'] for row in rows))
        self.assertTrue(all(row['predicted_cfg_gain'] == 2 for row in rows))


if __name__ == '__main__':
    unittest.main()
