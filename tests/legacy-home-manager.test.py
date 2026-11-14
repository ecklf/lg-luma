import importlib.util
from pathlib import Path
import unittest
import tempfile
from types import SimpleNamespace
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    'legacy_home', Path(__file__).resolve().parents[1] / 'tv' / 'legacy-home.py')
manager = importlib.util.module_from_spec(spec)
spec.loader.exec_module(manager)


class ConfigTests(unittest.TestCase):
    def setUp(self):
        self.current = [
            {'file': 'stock.js', 'handler': 'handleScreenSaver'},
            {'file': 'stock.js', 'handler': 'handleAccessibility'},
            {'file': 'stock.js', 'handler': 'handleSystemKeys'},
            {'file': 'owner.js', 'handler': 'ownerMapping'},
        ]
        self.writes = []

    def luna(self, service, method, payload):
        if method == 'getConfigs':
            return {'configs': {manager.KEY: self.current}}
        self.writes.append(payload)
        self.current = payload['configs'][manager.KEY]
        return {'returnValue': True}

    def test_insert_preserves_accessibility_and_other_mappings(self):
        original = list(self.current)
        with patch.object(manager, 'luna', self.luna):
            manager.configure(True)
        self.assertEqual(self.current[:2], original[:2])
        self.assertEqual(self.current[2]['file'], manager.FILTER)
        self.assertEqual(self.current[3:], original[2:])

    def test_enable_is_idempotent(self):
        with patch.object(manager, 'luna', self.luna):
            manager.configure(True)
            manager.configure(True)
        self.assertEqual(len(self.writes), 1)

    def test_disable_removes_only_our_filter_and_keeps_later_owner_changes(self):
        with patch.object(manager, 'luna', self.luna):
            manager.configure(True)
            self.current.append({'file': 'later.js', 'handler': 'newMapping'})
            manager.configure(False)
        self.assertEqual(len(self.current), 5)
        self.assertEqual(self.current[-1]['file'], 'later.js')
        self.assertFalse(any(entry['file'] == manager.FILTER for entry in self.current))

    def test_config_error_propagates(self):
        with patch.object(manager, 'luna', side_effect=RuntimeError('offline')):
            with self.assertRaisesRegex(RuntimeError, 'offline'):
                manager.configure(True)

    def test_unknown_filter_layout_is_not_overwritten(self):
        self.current = [{'file': 'unknown.js', 'handler': 'unknown'}]
        with patch.object(manager, 'luna', self.luna):
            with self.assertRaisesRegex(RuntimeError, 'compatible LG'):
                manager.configure(True)
        self.assertEqual(self.writes, [])

    def test_symlink_state_directory_and_files_are_rejected(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            outside = root / 'outside'
            outside.mkdir()
            state = root / 'state'
            state.symlink_to(outside, target_is_directory=True)
            with patch.object(manager, 'STATE', str(state)):
                with self.assertRaisesRegex(RuntimeError, 'root-owned directory'):
                    manager.prepare_state()
            state.unlink()
            state.mkdir()
            (state / 'legacy-home.py').symlink_to(outside / 'untouched')
            original_lstat = manager.os.lstat
            root_directory = SimpleNamespace(st_mode=state.lstat().st_mode, st_uid=0)
            with patch.object(manager, 'STATE', str(state)), patch.object(
                    manager.os, 'lstat',
                    lambda path: root_directory if path == str(state) else original_lstat(path)):
                with self.assertRaisesRegex(RuntimeError, 'Home state'):
                    manager.prepare_state()
            self.assertFalse((outside / 'untouched').exists())


if __name__ == '__main__':
    unittest.main()
