#!/usr/bin/env python3
"""Validate the JUCE 9.0.4 class inventory and the proposed parity-ledger seed.

Standard library only. Run either way, from anywhere:

    python3 parity/inventory/validate.py          # prints a summary, exit 0 if valid
    python3 -m unittest parity/inventory/validate.py

Checks (named after the WP-0.1-06 behaviours):
  b1  inventory groups classes by module and records a docs.juce.com source URL per module;
      every module listed in master plan §3 appears; a module marked complete lists classes.
  b2  the seed validates against parity/ledger.schema.json; rows are `todo` unless a gap;
      owner_wp names an existing work package; every seed row is in the inventory.
  b3  the deliberate gaps of master plan §1 are present with status `gap` and a reason.
  b4  summaries are single original lines (short, no doc-style boilerplate).
  b5  the seed does not touch parity/ledger.json (that file is generated in the merge queue).
"""
import json
import os
import re
import sys
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "..", ".."))
INVENTORY = os.path.join(HERE, "juce-9.0.4-classes.json")
SEED = os.path.join(HERE, "ledger-seed.json")
SCHEMA = os.path.join(REPO, "parity", "ledger.schema.json")
PLAN = os.path.join(REPO, "docs", "design", "master-plan.md")
PACKAGES = os.path.join(REPO, "work", "packages")


def load(path):
    with open(path, encoding="utf-8") as f:
        return json.load(f)


# --- minimal JSON Schema (draft 2020-12 subset used by ledger.schema.json) ---------------

def _type_ok(value, name):
    return {
        "object": isinstance(value, dict),
        "array": isinstance(value, list),
        "string": isinstance(value, str),
        "boolean": isinstance(value, bool),
        "integer": isinstance(value, int) and not isinstance(value, bool),
        "number": isinstance(value, (int, float)) and not isinstance(value, bool),
        "null": value is None,
    }[name]


def _resolve(root, ref):
    if not ref.startswith("#/"):
        raise ValueError("only local $ref supported: " + ref)
    node = root
    for part in ref[2:].split("/"):
        node = node[part]
    return node


def schema_errors(value, schema, root=None, path="$"):
    """Return a list of human-readable violations of `schema` by `value`."""
    root = schema if root is None else root
    if "$ref" in schema:
        return schema_errors(value, _resolve(root, schema["$ref"]), root, path)
    errs = []
    if "type" in schema and not _type_ok(value, schema["type"]):
        return ["%s: expected %s" % (path, schema["type"])]
    if "const" in schema and value != schema["const"]:
        errs.append("%s: expected const %r" % (path, schema["const"]))
    if "enum" in schema and value not in schema["enum"]:
        errs.append("%s: %r not in %r" % (path, value, schema["enum"]))
    if "pattern" in schema and isinstance(value, str) and not re.search(schema["pattern"], value):
        errs.append("%s: %r does not match %s" % (path, value, schema["pattern"]))
    if isinstance(value, dict):
        props = schema.get("properties", {})
        for key in schema.get("required", []):
            if key not in value:
                errs.append("%s: missing required %r" % (path, key))
        for key, sub in value.items():
            if key in props:
                errs += schema_errors(sub, props[key], root, "%s.%s" % (path, key))
            elif schema.get("additionalProperties") is False:
                errs.append("%s: unexpected property %r" % (path, key))
    if isinstance(value, list) and "items" in schema:
        for i, item in enumerate(value):
            errs += schema_errors(item, schema["items"], root, "%s[%d]" % (path, i))
    return errs


# --- helpers ----------------------------------------------------------------------------

def plan_modules():
    """Module names from the master plan §3 table (rows starting with | `juce_...`)."""
    text = open(PLAN, encoding="utf-8").read()
    start = text.index("## 3. ")
    end = text.index("## 4. ", start)
    return re.findall(r"^\| `(juce_[a-z0-9_]+)`", text[start:end], flags=re.M)


def all_rows(rows):
    for r in rows:
        yield r
        yield from all_rows(r.get("features", []))


# Modules whose name the ledger schema's id pattern cannot express yet (digit in the name).
# They stay in the inventory but are left out of the seed until the schema is amended.
SCHEMA_BLOCKED_MODULES = {"juce_box2d"}

REQUIRED_GAPS = {
    "juce_audio_processors/VSTPluginFormat": "VST2",
    "juce_audio_processors_headless/VSTPluginFormatHeadless": "VST2",
    "juce_audio_processors_headless/VST2ClientExtensions": "VST2",
    "juce_audio_utils/AudioCDBurner": "AudioCD",
    "juce_audio_utils/AudioCDReader": "AudioCD",
    "juce_gui_extra/ActiveXControlComponent": "ActiveX",
    "juce_gui_extra/AppleRemoteDevice": "AppleRemote",
    "juce_gui_basics/Component::runModalLoop": "modal",
    "tooling/Projucer": "Projucer",
}


class InventoryTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.inv = load(INVENTORY)
        cls.seed = load(SEED)
        cls.schema = load(SCHEMA)
        cls.mods = {m["module"]: m for m in cls.inv["modules"]}

    def b1_check(self):
        self.assertEqual(self.inv["baseline"], "9.0.4")
        self.assertIsInstance(self.inv["final"], bool)
        names = [m["module"] for m in self.inv["modules"]]
        self.assertEqual(len(names), len(set(names)), "duplicate module")
        for m in self.inv["modules"]:
            self.assertTrue(m["source_url"].startswith("https://docs.juce.com/"), m["module"])
            self.assertIsInstance(m["complete"], bool)
            if m["complete"]:
                self.assertTrue(m["classes"], "complete module lists no classes: " + m["module"])
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

    def test_b2_seed_validates_against_ledger_schema(self):
        self.assertEqual(schema_errors(self.seed, self.schema), [])
        ids = [r["id"] for r in all_rows(self.seed["rows"])]
        self.assertEqual(len(ids), len(set(ids)), "duplicate row id")
        known_wps = {f[:-5] for f in os.listdir(PACKAGES) if f.endswith(".toml")}
        inv_ids = {"%s/%s" % (m["module"], c["class"]) for m in self.inv["modules"]
                   for c in m["classes"] if m["module"] not in SCHEMA_BLOCKED_MODULES}
        for r in self.seed["rows"]:
            self.assertIn(r["status"], ("todo", "gap"), r["id"])
            if "owner_wp" in r:
                self.assertIn(r["owner_wp"], known_wps, r["id"])
            if r["juce"]["module"] != "tooling":
                self.assertIn(r["id"], inv_ids, "seed row not in inventory")
        self.assertEqual(inv_ids - set(ids), set(), "inventory class without a seed row")
        for mod in SCHEMA_BLOCKED_MODULES:
            self.assertIn("schema", self.mods[mod].get("note", ""), mod)

    def test_b3_deliberate_gaps_marked_with_reason(self):
        rows = {r["id"]: r for r in all_rows(self.seed["rows"])}
        for rid in REQUIRED_GAPS:
            self.assertIn(rid, rows, "missing gap row")
            self.assertEqual(rows[rid]["status"], "gap", rid)
        for r in rows.values():
            if r["status"] in ("gap", "deviation"):
                self.assertTrue(r.get("reason", "").strip(), r["id"])

    def test_b4_summaries_are_single_short_lines(self):
        for m in self.inv["modules"]:
            for c in m["classes"]:
                s = c["summary"]
                self.assertTrue(s.strip(), c["class"])
                self.assertNotIn("\n", s)
                self.assertLessEqual(len(s), 100, c["class"])
                # Doxygen-style boilerplate would suggest copied prose.
                self.assertNotRegex(s, r"(?i)^(this class|a class (which|that)|represents a)", c["class"])

    def test_b5_ledger_json_is_left_to_the_merge_queue(self):
        self.assertFalse(os.path.exists(os.path.join(HERE, "ledger.json")))
        self.assertNotEqual(os.path.abspath(SEED), os.path.join(REPO, "parity", "ledger.json"))


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
