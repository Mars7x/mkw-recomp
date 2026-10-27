#!/usr/bin/env python3
"""Complete remaining-set sharing keeps all existing gates and tests intact."""
import collections,unittest
from rank_blockers import candidates
class RemainingSetRanking(unittest.TestCase):
    def test_five_complete_sets_share_one_gate(self):
        buckets=collections.Counter({frozenset(['pacib']):1,frozenset(['sqrshl']):1,frozenset(['sqdmlal','sqdmlal2']):2,frozenset(['sadalp','srshl']):1,frozenset(['rshrn','rshrn2','srshl']):1,frozenset(['runtime:svc']):79})
        rows=candidates(buckets,15)
        full=next(r for r in rows if set(r['fixes'])=={'pacib','sqrshl','sqdmlal','sqdmlal2','sadalp','srshl','rshrn','rshrn2'})
        self.assertEqual(full['predicted_cfg_gain'],6)
        self.assertEqual(full['implementation_minutes'],[13,33])
        self.assertEqual(full['validation_minutes'],15)
        self.assertEqual(rows[0]['fixes'],full['fixes'])
        self.assertFalse(any('runtime:svc'in r['fixes']for r in rows))
    def test_no_pure_isa_set_does_not_invent_cheap_gains(self):
        rows=candidates(collections.Counter({frozenset(['runtime:svc']):1,frozenset(['policy:fcvt-half.fixed-test']):2}),15)
        self.assertEqual(rows,[])
if __name__=='__main__':unittest.main()
