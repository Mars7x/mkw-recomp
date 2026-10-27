#!/usr/bin/env python3
"""Whole-set pair synergy supplements existing individual-combination tests."""
import collections,unittest
from rank_blockers import candidates,family_cost
SHA1=frozenset(['sha1c','sha1p','sha1m','sha1h','sha1su0','sha1su1'])
SHA256=frozenset(['sha256h','sha256h2','sha256su0','sha256su1'])
class WholeBundleRanking(unittest.TestCase):
    def test_pair_of_complete_sets_shares_one_gate(self):
        rows=candidates(collections.Counter({SHA1:5,SHA256:4,frozenset(['runtime:svc']):100}),15)
        both=next(r for r in rows if set(r['fixes'])==SHA1|SHA256)
        self.assertEqual(both['predicted_cfg_gain'],9)
        self.assertEqual(both['implementation_minutes'],[15,35])
        self.assertEqual(both['validation_minutes'],15)
        self.assertEqual(rows[0]['fixes'],both['fixes'])
        self.assertFalse(any('runtime:svc' in r['fixes'] for r in rows))
    def test_bundle_costs_include_each_full_family(self):
        low,high=family_cost(SHA1|SHA256)
        for family in [SHA1,SHA256]:
            own=family_cost(family)
            self.assertGreaterEqual(low,own[0]);self.assertGreaterEqual(high,own[1])
    def test_three_complete_sets_share_the_gate(self):
        rows=candidates(collections.Counter({SHA1:5,SHA256:4,frozenset(['frecps']):1}),15)
        combined=next(r for r in rows if set(r['fixes'])==SHA1|SHA256|{'frecps'})
        self.assertEqual(combined['predicted_cfg_gain'],10)
        self.assertEqual(combined['implementation_minutes'],[17,40])
        self.assertEqual(combined['validation_minutes'],15)
        self.assertEqual(rows[0]['fixes'],combined['fixes'])
if __name__=='__main__':unittest.main()
