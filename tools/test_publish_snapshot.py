#!/usr/bin/env python3
"""Synthetic tests for source validation/copy identity; no GitHub actions."""
from pathlib import Path
import tempfile
import unittest
from publish import fingerprint_files, require_snapshot


class SnapshotTests(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory(prefix='mkw-publish-snapshot-')
        self.addCleanup(self.folder.cleanup)
        self.root = Path(self.folder.name)
        self.first = self.root / 'original'
        self.first.write_bytes(b'original source\n')
        self.files = {Path('docs/example.md'): self.first}
        self.frozen = fingerprint_files(self.files)

    def test_stable_sources_and_identical_copy_pass(self):
        require_snapshot(self.frozen, self.files)
        copied = self.root / 'copied'
        copied.write_bytes(self.first.read_bytes())
        require_snapshot(self.frozen, {Path('docs/example.md'): copied})

    def test_same_length_source_mutation_refuses(self):
        self.first.write_bytes(b'modified source\n')
        with self.assertRaisesRegex(SystemExit, 'Source snapshot changed'):
            require_snapshot(self.frozen, self.files)

    def test_added_or_removed_source_refuses(self):
        with self.assertRaises(SystemExit):
            require_snapshot(self.frozen, {})
        added = dict(self.files)
        added[Path('docs/additional.md')] = self.first
        with self.assertRaises(SystemExit):
            require_snapshot(self.frozen, added)

    def test_changed_copy_refuses_even_when_source_is_unchanged(self):
        copied = self.root / 'copied'
        copied.write_bytes(b'changed copy\n')
        with self.assertRaises(SystemExit):
            require_snapshot(self.frozen, {Path('docs/example.md'): copied})
        require_snapshot(self.frozen, self.files)


if __name__ == '__main__':
    unittest.main()
