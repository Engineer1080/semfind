"""Verify GUI locale selection and catalog coverage without a GTK display."""
import ast
from pathlib import Path
from string import Formatter
import unittest

import i18n


def fields(message):
    return {field for _, field, _, _ in Formatter().parse(message) if field is not None}


class LocalizationTests(unittest.TestCase):
    def test_follows_cli_locale_precedence(self):
        for environment, expected in (
            ({"LC_ALL": "C.UTF-8", "LANGUAGE": "de"}, "en"),
            ({"LC_MESSAGES": "zh_CN.UTF-8", "LANG": "de_DE.UTF-8"}, "zh"),
            ({"LC_ALL": "tr_TR.UTF-8", "LC_MESSAGES": "de"}, "tr"),
            ({"LANG": "pt_BR.UTF-8", "LANGUAGE": "xx:ja:en"}, "ja"),
            ({"LANG": "xx_XX.UTF-8"}, "en"),
            ({}, "en"),
        ):
            with self.subTest(environment=environment):
                self.assertEqual(i18n.message_language(environment), expected)

    def test_all_messages_and_placeholders_are_translated(self):
        source = Path(__file__).with_name("app.py").read_text(encoding="utf-8")
        tree = ast.parse(source)
        expected = {"Custom ONNX model", "Model name unavailable"}
        expected.update(
            node.args[0].value for node in ast.walk(tree)
            if isinstance(node, ast.Call) and isinstance(node.func, ast.Name)
            and node.func.id == "_" and node.args and isinstance(node.args[0], ast.Constant)
        )
        languages = "de es fr it ja nl pl pt ru tr zh".split()
        self.assertEqual(sorted(path.stem for path in i18n.LOCALES.glob("*.json")), languages)
        for language in languages:
            catalog = i18n.load_messages({"LANG": language})
            with self.subTest(language=language):
                self.assertEqual(set(catalog), expected)
                for message, translated in catalog.items():
                    self.assertTrue(translated.strip(), message)
                    self.assertEqual(fields(translated), fields(message), message)


if __name__ == "__main__":
    unittest.main()
