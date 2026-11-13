"""Put a TeamSpeak 3 identity into ~/.ts3client/settings.db, as the default.

Usage: ts3-identity.py <identity file> <settings.db> <fallback nickname>

The identity file is either an identity exported from TeamSpeak (Tools >
Identities > Export: an .ini with identity="...", nickname=, id=) or just the
identity string itself. An empty or missing file does nothing.

TS3 3.6 keeps identities (and bookmarks etc.) in the ProtobufItems table, one
protobuf message per row, plus a "Checksum" row: the SHA-1 of every other row's
value, concatenated in key order. An identity row looks like
  2: item uuid  4: 0  5: "ffffffff-..."  6: 1 (identity)  7: ""  8: ""
  9: unix time  17: { 1: identity  2: name  3: nickname  4: phonetic
                      5: 1 when it's the default }
Runs before the client starts. Once the identity is there (whatever you then
change about it in TeamSpeak) it does nothing.
"""

import hashlib
import os
import re
import sqlite3
import sys
import time
import uuid

IDENTITY_TYPE = 1
NO_OWNER = "ffffffff-ffff-ffff-ffff-ffffffffffff"


# --- just enough protobuf --------------------------------------------------


def varint(n):
    out = bytearray()
    while True:
        b = n & 0x7F
        n >>= 7
        if n:
            out.append(b | 0x80)
        else:
            out.append(b)
            return bytes(out)


def read_varint(buf, i):
    n = shift = 0
    while True:
        b = buf[i]
        i += 1
        n |= (b & 0x7F) << shift
        shift += 7
        if not b & 0x80:
            return n, i


def fields(buf):
    """[(field, wire type, value)]: ints for varints, bytes otherwise."""
    out, i = [], 0
    while i < len(buf):
        key, i = read_varint(buf, i)
        f, wt = key >> 3, key & 7
        if wt == 0:
            v, i = read_varint(buf, i)
        elif wt == 2:
            n, i = read_varint(buf, i)
            v, i = bytes(buf[i : i + n]), i + n
        elif wt == 1:
            v, i = bytes(buf[i : i + 8]), i + 8
        elif wt == 5:
            v, i = bytes(buf[i : i + 4]), i + 4
        else:
            raise ValueError(f"wire type {wt}")
        out.append((f, wt, v))
    return out


def encode(fs):
    out = bytearray()
    for f, wt, v in fs:
        out += varint(f << 3 | wt)
        if wt == 0:
            out += varint(v)
        elif wt == 2:
            if isinstance(v, str):
                v = v.encode()
            out += varint(len(v)) + v
        else:
            out += v
    return bytes(out)


def get(fs, f):
    return next((v for ff, _, v in fs if ff == f), None)


# --- the identity ----------------------------------------------------------


def parse(text):
    """(identity, name, nickname) from an exported .ini or a bare string."""
    text = text.strip()
    if "identity=" not in text:
        return text, None, None
    vals = {}
    for line in text.splitlines():
        m = re.match(r"\s*([a-z_]+)\s*=\s*(.*?)\s*$", line)
        if m:
            vals[m[1]] = m[2].strip('"')
    return vals.get("identity", ""), vals.get("id") or None, vals.get("nickname") or None


def main():
    src, db_path, fallback_nick = sys.argv[1:4]
    try:
        with open(src) as f:
            identity, name, nick = parse(f.read())
    except FileNotFoundError:
        return
    if not identity:
        return
    name = name or "vainnor"
    nick = nick or fallback_nick

    os.makedirs(os.path.dirname(db_path), exist_ok=True)
    db = sqlite3.connect(db_path)
    with db:
        db.execute(
            "CREATE TABLE IF NOT EXISTS ProtobufItems (timestamp INTEGER UNSIGNED NOT NULL,"
            " key VARCHAR NOT NULL UNIQUE, value VARCHAR)"
        )
        db.execute("CREATE INDEX IF NOT EXISTS index_ProtobufItems_key ON ProtobufItems (key)")
        rows = {
            k: bytes(v)
            for k, v in db.execute("SELECT key, value FROM ProtobufItems WHERE key != 'Checksum'")
        }

        now = int(time.time())
        changed = {}
        for k, v in rows.items():
            item = fields(v)
            if get(item, 6) != IDENTITY_TYPE or get(item, 17) is None:
                continue
            ident = fields(get(item, 17))
            if get(ident, 1) == identity.encode():
                return  # already there: leave it (and which one is default) alone
            if get(ident, 5):
                # Ours becomes the default instead.
                ident = [(f, wt, val) for f, wt, val in ident if f != 5] + [(5, 0, 0)]
                item = [
                    (f, wt, encode(ident) if f == 17 else (now if f == 9 else val))
                    for f, wt, val in item
                ]
                changed[k] = encode(item)

        ident = [(1, 2, identity), (2, 2, name), (3, 2, nick), (4, 2, ""), (5, 0, 1)]
        item = [
            (2, 2, str(uuid.uuid4())),
            (4, 0, 0),
            (5, 2, NO_OWNER),
            (6, 0, IDENTITY_TYPE),
            (7, 2, ""),
            (8, 2, ""),
            (9, 0, now),
            (17, 2, encode(ident)),
        ]
        key = str(max((int(k) for k in rows if k.isdigit()), default=0) + 1)
        changed[key] = encode(item)

        for k, v in changed.items():
            db.execute(
                "INSERT INTO ProtobufItems (timestamp, key, value) VALUES (?, ?, ?)"
                " ON CONFLICT(key) DO UPDATE SET timestamp = excluded.timestamp, value = excluded.value",
                (now, k, v),
            )
            rows[k] = v
        # (The client only uses this to notice changes for myTeamSpeak sync; a
        # stale one is harmless.)
        order = sorted(rows, key=lambda k: (not k.isdigit(), int(k) if k.isdigit() else 0, k))
        checksum = hashlib.sha1(b"".join(rows[k] for k in order)).digest()
        db.execute(
            "INSERT INTO ProtobufItems (timestamp, key, value) VALUES (?, 'Checksum', ?)"
            " ON CONFLICT(key) DO UPDATE SET timestamp = excluded.timestamp, value = excluded.value",
            (now, checksum),
        )
    print("ts3-identity: added the identity as the default", file=sys.stderr)


main()
