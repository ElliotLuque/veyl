"""Pruebas del helper sin acceder al portapapeles ni al estado del usuario."""
import contextlib
import importlib.machinery
import importlib.util
import io
import json
import os
from pathlib import Path
import struct
import subprocess
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
loader = importlib.machinery.SourceFileLoader("veyl_clip", str(ROOT / "bin/veyl-clip"))
spec = importlib.util.spec_from_loader(loader.name, loader)
clip = importlib.util.module_from_spec(spec)
loader.exec_module(clip)


class ClipboardTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        clip.HOME = self.temp.name
        clip.DATA = os.path.join(clip.HOME, "data")
        clip.INDEX = os.path.join(clip.HOME, "index.json")
        self.output = io.StringIO()
        self.redirect = contextlib.redirect_stdout(self.output)
        self.redirect.__enter__()
        self.addCleanup(self.redirect.__exit__, None, None, None)

    def store(self, blob=b"hola", state="data", mime=b"text/plain", stream=None):
        stream = stream or io.BytesIO(blob)
        stdin = type("Input", (), {"buffer": stream})()
        def paste(*args, **kwargs):
            return mime if args == ("--list-types",) else blob
        with patch.object(clip.sys, "stdin", stdin), patch.object(clip, "paste", side_effect=paste), patch.dict(os.environ, {"CLIPBOARD_STATE": state}):
            clip.store()

    def entries(self):
        with clip.index(notify=False) as entries:
            return list(entries)

    def test_first_list_is_only_json(self):
        with patch.object(clip.sys, "argv", ["veyl-clip", "list"]):
            clip.main()
        self.assertEqual(json.loads(self.output.getvalue()), [])

    def test_short_png_does_not_crash(self):
        self.assertEqual(clip.measure(b"\x89PNG\r\n\x1a\n0000IHDR"), (0, 0))
        self.assertEqual(clip.measure(b"\x89PNG\r\n\x1a\n0000IHDR" + struct.pack(">II", 10, 20)), (10, 20))

    def test_deduplication_preserves_pins(self):
        self.store()
        wanted = self.entries()[0]["id"]
        clip.pin(wanted)
        self.store(b"otro")
        self.store()
        entries = self.entries()
        self.assertEqual(len(entries), 2)
        self.assertEqual(entries[0]["id"], wanted)
        self.assertTrue(entries[0]["pinned"])
        clip.clear()
        self.assertEqual(len(self.entries()), 1)

    def test_secret_data_is_never_stored(self):
        self.store(state="sensitive")
        self.store(mime=b"text/plain\nx-kde-passwordManagerHint")
        self.assertEqual(self.entries(), [])

    def test_large_input_is_drained_with_bounded_reads(self):
        class Bounded(io.BytesIO):
            def read(self, size=-1):
                self_test.assertGreaterEqual(size, 0)
                self_test.assertLessEqual(size, clip.TEXT_MAX + 1)
                return super().read(size)
        self_test = self
        stream = Bounded(b"x" * (clip.TEXT_MAX * 3))
        self.store(stream=stream)
        self.assertEqual(stream.tell(), clip.TEXT_MAX * 3)
        self.assertEqual(self.entries(), [])

    def test_permissions_are_private(self):
        self.store()
        entry = self.entries()[0]
        for path in [clip.HOME, clip.DATA]:
            self.assertEqual(os.stat(path).st_mode & 0o777, 0o700)
        for path in [clip.INDEX, os.path.join(clip.DATA, entry["id"])]:
            self.assertEqual(os.stat(path).st_mode & 0o777, 0o600)

    def test_malformed_index_is_preserved(self):
        for content in ['null', '{}', '[null]', '[{"id":"../../victim"}]', '{broken']:
            Path(clip.INDEX).write_text(content)
            with self.assertRaises(ValueError):
                clip.clear()
            self.assertEqual(Path(clip.INDEX).read_text(), content)

    def test_copy_keeps_open_file_if_entry_is_deleted(self):
        self.store()
        wanted = self.entries()[0]["id"]
        def run(*args, **kwargs):
            clip.delete(wanted)
            self.assertEqual(kwargs["stdin"].read(), b"hola")
            return subprocess.CompletedProcess(args, 0)
        with patch.object(clip.subprocess, "run", side_effect=run):
            clip.copy(wanted)
        self.assertEqual(self.entries(), [])

    def test_history_is_bounded_but_pins_survive(self):
        with patch.object(clip, "KEEP", 2):
            self.store(b"fijado")
            clip.pin(self.entries()[0]["id"])
            for text in [b"uno", b"dos", b"tres"]:
                self.store(text)
        self.assertEqual(len(self.entries()), 3)
        self.assertEqual(len(os.listdir(clip.DATA)), 3)


if __name__ == "__main__":
    unittest.main()
