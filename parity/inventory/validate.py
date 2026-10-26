#!/usr/bin/env python3
"""Validate the JUCE 9.0.4 class inventory.

Standard library only. Run either way, from anywhere:

    python3 parity/inventory/validate.py          # prints a summary, exit 0 if valid
    python3 -m unittest parity/inventory/validate.py

Checks (named after the WP-0.1-06 behaviours):
  b1  inventory groups classes by module and records a docs.juce.com source URL per module;
      every module listed in master plan §3 appears.
  b4  summaries are single original lines (short, no doc-style boilerplate).
"""
import json
import os
import re
import sys
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "..", ".."))
INVENTORY = os.path.join(HERE, "juce-9.0.4-classes.json")
PLAN = os.path.join(REPO, "docs", "design", "master-plan.md")


def load(path):
    with open(path, encoding="utf-8") as f:
        return json.load(f)


# --- helpers ----------------------------------------------------------------------------

def plan_modules():
    """Module names from the master plan §3 table (rows starting with | `juce_...`)."""
    text = open(PLAN, encoding="utf-8").read()
    start = text.index("## 3. ")
    end = text.index("## 4. ", start)
    return re.findall(r"^\| `(juce_[a-z0-9_]+)`", text[start:end], flags=re.M)


class InventoryTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.inv = load(INVENTORY)
        cls.mods = {m["module"]: m for m in cls.inv["modules"]}

    def b1_check(self):
        self.assertEqual(self.inv["baseline"], "9.0.4")
        self.assertIsInstance(self.inv["final"], bool)
        names = [m["module"] for m in self.inv["modules"]]
        self.assertEqual(len(names), len(set(names)), "duplicate module")
        for m in self.inv["modules"]:
            self.assertTrue(m["source_url"].startswith("https://docs.juce.com/"), m["module"])
            self.assertIsInstance(m["complete"], bool)
            if not m["complete"]:
                self.assertTrue(m.get("note"), "incomplete module needs a note: " + m["module"])
            seen = set()
            for c in m["classes"]:
                self.assertNotIn(c["class"], seen, "duplicate %s in %s" % (c["class"], m["module"]))
                seen.add(c["class"])
        if self.inv["final"]:
            self.assertTrue(all(m["complete"] for m in self.inv["modules"]))
        missing = [p for p in plan_modules() if p not in self.mods]
        self.assertEqual(missing, [], "modules from master plan §3 missing")

    def test_b1_inventory_groups_classes_by_module_with_source_urls(self):
        self.b1_check()

    def test_b4_summaries_are_single_short_lines(self):
        for m in self.inv["modules"]:
            for c in m["classes"]:
                s = c["summary"]
                self.assertTrue(s.strip(), c["class"])
                self.assertNotIn("\n", s)
                self.assertLessEqual(len(s), 100, c["class"])
                # Doxygen-style boilerplate would suggest copied prose.
                self.assertNotRegex(s, r"(?i)^(this class|a class (which|that)|represents a)", c["class"])


def main():
    result = unittest.TextTestRunner(verbosity=1).run(
        unittest.defaultTestLoader.loadTestsFromTestCase(InventoryTests))
    inv = load(INVENTORY)
    n = sum(len(m["classes"]) for m in inv["modules"])
    done = sum(1 for m in inv["modules"] if m["complete"])
    print("inventory: %d classes, %d/%d modules complete, final=%s"
          % (n, done, len(inv["modules"]), inv["final"]))
    return 0 if result.wasSuccessful() else 1


if __name__ == "__main__":
    sys.exit(main())
