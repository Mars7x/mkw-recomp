#!/usr/bin/env python3
"""Synthetic runtime dependency accounting; no original inputs or guest execution."""
import unittest
from survey_runtime_contracts import contract_key,contract_buckets,ranked_contract_sets
class RuntimeContracts(unittest.TestCase):
    def test_svc_identity_and_exclusive_contract(self):
        for immediate in [0,1,0x7fff,0xffff]:self.assertEqual(contract_key(0xd4000001|(immediate<<5),'svc',f'#{immediate}'),f'service:svc:{immediate:#06x}')
        with self.assertRaises(ValueError):contract_key(0,'svc','#0')
        self.assertEqual(contract_key(0,'ldaxr','x0, [x1]'),'memory:exclusive-monitor')
        self.assertEqual(contract_key(0,'mrs','x0, cntpct_el0'),'system-register:read:cntpct_el0')
    def test_all_contracts_required_and_other_barriers_stay_excluded(self):
        def row(words,edges=(),omitted=False):return{'cfg_decode_issues':[{'word':w}for w in words],'cfg_edge_issues':list(edges),'omitted':omitted}
        classify={1:'service:svc:0x0001',2:'memory:exclusive-monitor',3:None}.get
        buckets,barriers,presence,services,total=contract_buckets([row([1]),row([1,2]),row([2]),row([1,3]),row([1],[4]),row([2],omitted=True)],classify)
        self.assertEqual(total,6);self.assertEqual(sum(buckets.values()),3)
        self.assertEqual(barriers['structural'],2);self.assertEqual(barriers['other_isa_policy_opaque_or_control_flow'],1)
        self.assertEqual(presence['service:svc:0x0001'],4);self.assertEqual(services['service:svc:0x0001'],4)
        ranked=ranked_contract_sets(buckets);self.assertEqual(ranked[0]['conditional_extent_gain'],3)
        self.assertEqual(len(ranked[0]['contracts']),2);self.assertFalse(ranked[0]['runnable_functions_claimed'])
    def test_empty_contracts_cannot_create_candidates(self):self.assertEqual(ranked_contract_sets({}),[])
if __name__=='__main__':unittest.main()
