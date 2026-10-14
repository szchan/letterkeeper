#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Exact-test hosted TDD evidence runner (implementation pending)."""


def validate(commit, crate, test, phase):
    raise NotImplementedError


def command(crate, test, list_only=False):
    raise NotImplementedError


def execute(commit, crate, test, phase, runner, summary):
    raise NotImplementedError
