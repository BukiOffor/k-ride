"""Prepare embedded admin SQL against a migrated disposable PostgreSQL database.
Usage: python3 tests/validate_admin_sql.py postgresql://.../disposable_database
No application data is modified.
"""
import json
import re
import subprocess
import sys
from pathlib import Path

statements = []
for path in Path("src/admin/services").rglob("*.rs"):
    for match in re.finditer(r'"((?:[^"\\]|\\.)*)"', path.read_text()):
        source = match.group(1)
        if not source.lstrip().startswith(("SELECT ", "WITH ", "UPDATE ", "INSERT ", "DELETE ")):
            continue
        if "$2" not in source or "{predicate}" in source:
            continue
        # Ordinary Rust string escapes used here match JSON's string escapes.
        sql = json.loads('"' + source.replace('\n', '\\n') + '"')
        statements.append((path, sql))
script = "\n".join(
    f"PREPARE admin_check_{i}(jsonb,uuid) AS {sql};"
    for i, (_, sql) in enumerate(statements)
)
result = subprocess.run(
    ["psql", sys.argv[1], "-v", "ON_ERROR_STOP=1"], input=script, text=True, capture_output=True
)
if result.returncode:
    print(result.stdout)
    print(result.stderr)
    sys.exit(result.returncode)
print(f"Prepared {len(statements)} embedded admin queries successfully.")
