import json
import unittest
from unittest.mock import Mock

from galvanize import Client, Statement
from galvanize.client import admin_ping, admin_set_log_filter


class ClientTests(unittest.TestCase):
    def setUp(self):
        self.session = Mock()
        self.response = Mock()
        self.response.ok = True
        self.response.status_code = 200
        self.response.content = b'{"ok":true}'
        self.response.json.return_value = {"ok": True}
        self.response.headers = {}
        self.session.request.return_value = self.response
        self.client = Client("https://db.example/base", admin_url="https://admin.example",
                             highlow_url="https://control.example", bearer_token="secret",
                             session=self.session)

    def test_statement_wire_shapes_and_binary_parameters(self):
        self.assertEqual(Statement("SELECT ?", [b"\x00\xff"]).wire(), ["SELECT ?", [[0, 255]]])
        self.assertEqual(Statement("SELECT :value", named_params={"value": 3}).wire(),
                         {"query": "SELECT :value", "named_params": {"value": 3}})

    def test_query_uses_public_api_and_bearer_token(self):
        self.client.table_stats(["notes"])
        args = self.session.request.call_args
        self.assertEqual(args.args[:2], ("POST", "https://db.example/base/v1/table_stats"))
        self.assertEqual(args.kwargs["headers"]["Authorization"], "Bearer secret")
        self.assertEqual(args.kwargs["json"], {"tables": ["notes"]})

    def test_admin_command_is_forwarded_without_public_bearer_token(self):
        self.response.json.return_value = {"responses": [{"Success": None}]}
        self.client.run_admin_command(admin_ping())
        args = self.session.request.call_args
        self.assertEqual(args.args[1], "https://admin.example/v1/admin/commands")
        self.assertNotIn("Authorization", args.kwargs["headers"])
        self.assertEqual(args.kwargs["json"], "Ping")

    def test_admin_constructor_serialization(self):
        self.assertEqual(json.loads(admin_set_log_filter("info")), {"Log": {"Set": {"filter": "info"}}})

    def test_reject_non_https_unlock(self):
        client = Client("http://db.example")
        with self.assertRaisesRegex(ValueError, "HTTPS"):
            client.unlock_from_env()


if __name__ == "__main__":
    unittest.main()
