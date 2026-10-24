#!/usr/bin/env python3
"""Run the semantic-owner inventory from its maintained tools location."""
import pathlib
import runpy

runpy.run_path(str(pathlib.Path(__file__).resolve().parents[2] / "tools/value-algebra-inventory/inventory.py"),
               run_name="__main__")
