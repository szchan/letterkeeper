#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Synthetic readiness state-machine tests; no Codex, Cargo or network calls."""
import sys
import unittest

sys.dont_write_bytecode = True
from verify_codex import wait_for_mcp_servers


EXPECTED = {"letterkeeper_readonly", "letterkeeper_draft"}


def inventory(first="connected", second="connected", names=None, cursor=None):
    return {"data": [{"name": name, "runtimeStatus": status,
                      "toolsError": "synthetic-secret-must-not-be-logged"}
                     for name, status in zip(sorted(EXPECTED) if names is None else names,
                                             (first, second))], "nextCursor": cursor}


class Clock:
    def __init__(self):
        self.now = 0
        self.sleeps = []

    def monotonic(self):
        return self.now

    def sleep(self, seconds):
        self.sleeps.append(seconds)
        self.now += seconds


class StubClient:
    def __init__(self, *responses, clock=None, rpc_elapsed=0):
        self.responses = list(responses)
        self.calls = []
        self.clock = clock
        self.rpc_elapsed = rpc_elapsed

    def call(self, method, params, *, timeout):
        self.calls.append((method, params, timeout))
        if self.clock:
            self.clock.now += min(timeout, self.rpc_elapsed)
            if self.rpc_elapsed >= timeout:
                raise TimeoutError("codex_client_deadline")
        response = self.responses.pop(0) if len(self.responses) > 1 else self.responses[0]
        if isinstance(response, Exception):
            raise response
        return response


class ReadinessTests(unittest.TestCase):
    def wait(self, client, clock=None, timeout=30):
        clock = clock or Clock()
        return wait_for_mcp_servers(client, "synthetic-thread", EXPECTED, timeout=timeout,
                                    monotonic=clock.monotonic, sleep=clock.sleep)

    def test_connected_returns_without_sleep_or_extra_rpc(self):
        response = inventory()
        client, clock = StubClient(response), Clock()
        self.assertEqual(self.wait(client, clock), response["data"])
        self.assertEqual(clock.sleeps, [])
        self.assertEqual(client.calls, [("mcpServerStatus/list",
                         {"threadId": "synthetic-thread", "detail": "toolsAndAuthOnly"}, 30)])

    def test_not_started_and_starting_wait_until_both_connected(self):
        client = StubClient(inventory("notStarted", "starting"),
                            inventory("connected", "starting"), inventory())
        clock = Clock()
        self.assertEqual(self.wait(client, clock), inventory()["data"])
        self.assertEqual(len(client.calls), 3)
        self.assertTrue(all(0 < duration <= 1 for duration in clock.sleeps))
        self.assertEqual(len(clock.sleeps), 2)
        self.assertTrue(all(call[0] == "mcpServerStatus/list" for call in client.calls))
        self.assertLess(client.calls[-1][2], 30)

    def test_terminal_and_unavailable_statuses_fail_immediately_without_leaking_detail(self):
        for status in ("failed", "authenticationRequired", "cancelled", "disabled", None,
                       "unknown", "synthetic-secret-must-not-be-logged"):
            with self.subTest(status=status):
                client, clock = StubClient(inventory("starting", status), inventory()), Clock()
                with self.assertRaisesRegex(RuntimeError, "codex_mcp_readiness_rejected") as error:
                    self.wait(client, clock)
                self.assertNotIn("synthetic-secret", str(error.exception))
                self.assertEqual(len(client.calls), 1)
                self.assertEqual(clock.sleeps, [])

    def test_inventory_missing_extra_duplicate_and_pagination_fail_before_wait(self):
        bad = [inventory(names=["letterkeeper_readonly"]),
               inventory(names=["letterkeeper_readonly", "unexpected"]),
               inventory(names=["letterkeeper_readonly", "letterkeeper_readonly"]),
               inventory(cursor="unexpected-more")]
        extra = inventory()
        extra["data"].append({"name": "unexpected", "runtimeStatus": "starting"})
        bad.append(extra)
        for response in bad:
            with self.subTest(response=response):
                client, clock = StubClient(response, inventory()), Clock()
                with self.assertRaisesRegex(RuntimeError, "unexpected_server_inventory"):
                    self.wait(client, clock)
                self.assertEqual(len(client.calls), 1)
                self.assertEqual(clock.sleeps, [])

    def test_inventory_is_checked_again_on_each_poll(self):
        client = StubClient(inventory("starting"), inventory(names=["letterkeeper_readonly"]))
        with self.assertRaisesRegex(RuntimeError, "unexpected_server_inventory"):
            self.wait(client)
        self.assertEqual(len(client.calls), 2)

    def test_pending_times_out_within_bound_and_no_rpc_after_deadline(self):
        clock = Clock()
        client = StubClient(inventory("starting", "notStarted"))
        with self.assertRaisesRegex(TimeoutError, "codex_mcp_readiness_deadline"):
            self.wait(client, clock, timeout=2.5)
        self.assertEqual(clock.now, 2.5)
        self.assertTrue(all(0 < call[2] <= 2.5 for call in client.calls))
        self.assertEqual(clock.sleeps[-1], .5)

    def test_rpc_time_consumes_the_same_readiness_deadline(self):
        clock = Clock()
        client = StubClient(inventory("starting"), clock=clock, rpc_elapsed=2)
        with self.assertRaises(TimeoutError):
            self.wait(client, clock, timeout=3.5)
        self.assertEqual(clock.now, 3.5)
        self.assertEqual(client.calls[-1][2], .5)

    def test_rpc_failure_is_not_retried(self):
        for error in (RuntimeError("codex_rpc_rejected"), TimeoutError("codex_client_deadline")):
            with self.subTest(error=error):
                client, clock = StubClient(error, inventory()), Clock()
                with self.assertRaises(type(error)) as actual:
                    self.wait(client, clock)
                self.assertIs(actual.exception, error)
                self.assertEqual(len(client.calls), 1)
                self.assertEqual(clock.sleeps, [])


if __name__ == "__main__":
    unittest.main()
