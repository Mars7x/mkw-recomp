#!/usr/bin/env python3
"""Additional five-fix and shared-effort checks; existing tests unchanged."""
import collections
import unittest
from rank_blockers import candidates, family_cost
class FiveFixRanking(unittest.TestCase):
    def test_five_fixes_share_one_validation(self):
        buckets=collections.Counter({frozenset(['pmull','pmull2']):3,frozenset(['uabd','uabdl']):2,frozenset(['fcvtl']):1,frozenset(['runtime:svc']):100})
        rows=candidates(buckets,15)
        package=next(r for r in rows if r['fixes']==['fcvtl','pmull','pmull2','uabd','uabdl'])
        self.assertEqual(package['predicted_cfg_gain'],6)
        self.assertEqual(package['validation_minutes'],15)
        self.assertEqual(package['implementation_minutes'],[17,43])
        self.assertEqual(rows[0]['fixes'],package['fixes'])
        self.assertFalse(any('runtime:svc' in r['fixes'] for r in rows))
    def test_shared_families_cost_more_than_either_single(self):
        for pair in [('pmull','pmull2'),('uabd','uabdl')]:
            low,high=family_cost(pair)
            for name in pair:
                single=family_cost([name])
                self.assertGreaterEqual(low,single[0]);self.assertGreaterEqual(high,single[1])
if __name__=='__main__':unittest.main()
