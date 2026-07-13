#!/usr/bin/env python3

from __future__ import annotations

from pathlib import Path
import sys
import tempfile
import unittest


BENCHES_DIR = Path(__file__).resolve().parent
if str(BENCHES_DIR) not in sys.path:
    sys.path.insert(0, str(BENCHES_DIR))

import bench_data


class EvidenceDocumentLoadingTests(unittest.TestCase):
    def write_document(self, directory: Path, name: str, content: str) -> Path:
        path = directory / name
        path.write_text(content, encoding="utf-8")
        return path

    def test_loads_valid_object_without_changing_its_contents(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = self.write_document(
                Path(directory),
                "speed-0-test.json",
                '{"title":"test","order":0,"clocks":{"tach":{"now":1,"elapsed":2}}}',
            )

            self.assertEqual(
                bench_data.load_json_document(path),
                {
                    "title": "test",
                    "order": 0,
                    "clocks": {"tach": {"now": 1, "elapsed": 2}},
                },
            )

    def test_rejects_duplicate_top_level_key(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = self.write_document(
                Path(directory),
                "speed-0-test.json",
                '{"title":"first","title":"second"}',
            )

            with self.assertRaisesRegex(ValueError, r"duplicate JSON key 'title'"):
                bench_data.load_json_document(path)

    def test_rejects_duplicate_nested_key(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = self.write_document(
                Path(directory),
                "speed-0-test.json",
                '{"clocks":{"tach":{"now":1,"now":2}}}',
            )

            with self.assertRaisesRegex(ValueError, r"duplicate JSON key 'now'"):
                bench_data.load_json_document(path)

    def test_primary_cell_loader_propagates_duplicate_key_rejection(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            self.write_document(
                Path(directory),
                "speed-0-test.json",
                '{"order":0,"clocks":{},"clocks":{}}',
            )

            with self.assertRaisesRegex(ValueError, r"duplicate JSON key 'clocks'"):
                bench_data.load_cell_documents(directory)


if __name__ == "__main__":
    unittest.main()
