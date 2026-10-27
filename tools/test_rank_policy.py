#!/usr/bin/env python3
"""Preserved half-conversion exclusions apply to full arrangements."""
import unittest
from rank_blockers import fixed_half_conversion_policy
class FixedConversionPolicy(unittest.TestCase):
    def test_all_register_pairs_share_each_excluded_arrangement(self):
        for base in [0x1ee24000,0x1e23c000]:
            for dst in range(32):
                for src in range(32):
                    self.assertTrue(fixed_half_conversion_policy(base|(src<<5)|dst))
    def test_other_precisions_keep_their_existing_classification(self):
        for base in [0x1e22c000,0x1e624000,0x0e617800,0x1ee2c000,0x1e63c000]:
            self.assertFalse(fixed_half_conversion_policy(base))
if __name__=='__main__':unittest.main()
