"""Exercise actual password-reset SQL on a disposable PostgreSQL database.
Usage: python3 tests/validate_password_reset_sql.py postgresql://.../disposable_db
Creates minimal users/outbox fixtures; refuses a database already containing users.
"""
import json
import re
import subprocess
import sys
from pathlib import Path

source = Path('src/auth/password_reset.rs').read_text()
issue = re.search(r'r#"(WITH issued AS .*?)"#', source, re.S).group(1)
outbox = re.search(r'r#"(INSERT INTO email_outbox.*?)"#', source, re.S).group(1)
lookup = re.search(r'r#"(SELECT COALESCE\(\(SELECT jsonb_build_object\(\'challenge\'.*?)"#', source, re.S).group(1)
mutations = re.findall(r'"(UPDATE (?:password_reset_otps|users) SET .*?)"', source)
uid = '00000000-0000-7000-8000-000000000001'
c1 = '00000000-0000-7000-8000-000000000002'
c2 = '00000000-0000-7000-8000-000000000003'

def execute(name, payload):
    return f"EXECUTE {name}($p${json.dumps(payload)}$p$::jsonb,'{uid}'::uuid);"

def check(condition):
    return f"DO $$ BEGIN IF NOT ({condition}) THEN RAISE EXCEPTION 'reset assertion failed'; END IF; END $$;"

sql = "CREATE TABLE users(id uuid PRIMARY KEY,password_hash text,updated_at timestamp);\n"
sql += Path('migrations/2026-11-01-010000_password_reset/up.sql').read_text()
sql += "CREATE TABLE email_outbox(id uuid PRIMARY KEY,aggregate_id uuid,event_key text,recipient_id uuid REFERENCES users(id),recipient text,subject text,body text);\n"
sql += f"INSERT INTO users(id,password_hash) VALUES('{uid}','old');\n"
for name, statement in [('issue', issue), ('outbox', outbox), ('lookup', lookup), ('attempt', mutations[0]), ('password', mutations[1])]:
    sql += f'PREPARE {name}(jsonb,uuid) AS {statement};\n'
sql += execute('issue', {'challenge': c1, 'hash': 'digest1'})
sql += check("(SELECT attempts=0 AND consumed_at IS NULL AND expires_at>now() FROM password_reset_otps)")
sql += execute('attempt', {'valid': False})
sql += execute('issue', {'challenge': c2, 'hash': 'digest2'})
sql += check(f"(SELECT challenge_id='{c1}' AND attempts=1 FROM password_reset_otps)")
sql += "UPDATE password_reset_otps SET requested_at=now()-interval '61 seconds';"
sql += execute('issue', {'challenge': c2, 'hash': 'digest2'})
sql += check(f"(SELECT challenge_id='{c2}' AND attempts=1 FROM password_reset_otps)")
for _ in range(4):
    sql += execute('attempt', {'valid': False})
sql += check("(SELECT attempts=5 FROM password_reset_otps)")
sql += "UPDATE password_reset_otps SET requested_at=now()-interval '61 seconds';"
sql += execute('issue', {'challenge': c1, 'hash': 'blocked'})
sql += check(f"(SELECT challenge_id='{c2}' AND attempts=5 FROM password_reset_otps)")
sql += execute('lookup', {})
sql += "UPDATE password_reset_otps SET expires_at=now()-interval '1 second';"
sql += execute('issue', {'challenge': c1, 'hash': 'fresh'})
sql += check("(SELECT attempts=0 AND expires_at>now() FROM password_reset_otps)")
sql += execute('outbox', {'challenge': c1, 'email': 'test@example.invalid', 'body': 'Test OTP: 001234'})
sql += check("(SELECT recipient='test@example.invalid' AND body='Test OTP: 001234' FROM email_outbox)")
sql += execute('attempt', {'valid': True})
sql += execute('password', {'hash': 'new-argon2-hash'})
sql += check("(SELECT consumed_at IS NOT NULL AND attempts=1 FROM password_reset_otps)")
sql += check("(SELECT password_hash='new-argon2-hash' FROM users)")
sql += execute('lookup', {})
# Invalid/consumed challenges must not appear in the same predicate used by reset.
sql += check("NOT EXISTS(SELECT 1 FROM password_reset_otps WHERE expires_at>now() AND attempts<5 AND consumed_at IS NULL)")
sql += Path('migrations/2026-11-01-010000_password_reset/down.sql').read_text()
sql += 'DROP TABLE email_outbox; DROP TABLE users;'
result = subprocess.run(['psql', sys.argv[1], '-X', '-v', 'ON_ERROR_STOP=1'], input=sql, text=True, capture_output=True)
if result.returncode:
    print(result.stderr)
    sys.exit(result.returncode)
print('Password reset SQL checks passed: migration, cooldown, resend, attempt cap, expiry, consumption, password update, rollback migration.')
