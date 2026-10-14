# Copyright: Anki 3DS contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

"""Independent SQL oracle and exact desktop/native comparison for disposable inputs."""
import hashlib
import json
import sqlite3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "out/3ds/normal-sync-probe"
CASES = ["no_changes", "local", "remote", "both", "review", "deletion", "conflict",
         "interruption", "cancellation", "full_required"]
TABLES = ["notes", "cards", "revlog", "graves", "notetypes", "decks", "deck_config", "tags"]

def snapshot(path):
    db = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    db.create_collation("unicase", lambda a,b: (a.casefold()>b.casefold())-(a.casefold()<b.casefold()))
    result = {}
    for table in TABLES:
        cur = db.execute(f"select * from {table} order by 1,2")
        rows = [[{"blob": list(v)} if isinstance(v, bytes) else v for v in row] for row in cur.fetchall()]
        result[table] = {"columns": [c[0] for c in cur.description], "rows": rows}
    result["usn"] = db.execute("select usn from col").fetchone()[0]
    result["integrity"] = db.execute("pragma integrity_check").fetchone()[0]
    db.close()
    return result

def rows(s, table):
    return [dict(zip(s[table]["columns"], row)) for row in s[table]["rows"]]

def without_usn(row):
    return {k: v for k,v in row.items() if k != "usn"}

def assert_oracle(case):
    name = case["case"]
    left, right = case["before"], case["server_before"]
    actual = case["after"]
    assert actual["integrity"] == case["server"]["integrity"] == "ok", name
    if name in ("no_changes", "full_required"):
        assert actual == left and case["server"] == right, name
        assert case["calls"] == ["server_meta"], name
        return
    for table in ["notes", "cards", "revlog", "notetypes", "decks", "deck_config", "tags"]:
        assert actual[table] == case["server"][table], (name,table,"convergence")
    assert actual["usn"] == left["usn"]+1, name
    assert "server_finish" in case["calls"], name
    if name == "deletion":
        assert not actual["notes"]["rows"] and not actual["cards"]["rows"], name
        assert [without_usn(r) for r in rows(actual,"revlog")] == [without_usn(r) for r in rows(left,"revlog")], name
        assert len(actual["graves"]["rows"]) >= 4, name
    else:
        expected = rows(left, "notes")
        if name in ("remote", "conflict"):
            expected = rows(right, "notes")
        elif name == "both":
            expected = [rows(left, "notes")[0], rows(right, "notes")[1]]
        assert [without_usn(r) for r in rows(actual,"notes")] == [without_usn(r) for r in expected], name
        if name == "conflict":
            assert rows(actual,"notes")[0]["flds"].startswith("conflict-server\x1f"), name
        if name == "review":
            expected_logs = {r["id"]: without_usn(r) for r in rows(left,"revlog")+rows(right,"revlog")}
            assert [without_usn(r) for r in rows(actual,"revlog")] == [expected_logs[k] for k in sorted(expected_logs)], name
            expected_cards = {r["id"]:r for r in rows(left,"cards")}
            for row in rows(right,"cards"):
                if row["reps"] > expected_cards[row["id"]]["reps"]:
                    expected_cards[row["id"]] = row
            assert [without_usn(r) for r in rows(actual,"cards")] == [without_usn(expected_cards[k]) for k in sorted(expected_cards)], name
            assert all(json.loads(r["data"]).get("s") is not None for r in rows(actual,"cards")), "FSRS missing"
        else:
            for table in ("cards","revlog"):
                assert [without_usn(r) for r in rows(actual,table)] == [without_usn(r) for r in rows(left,table)], (name,table)
    if name in ("interruption","cancellation"):
        for table in ("cards","notes","revlog","graves","usn"):
            assert case["rollback"]["client"][table] == left[table], (name,table,"client rollback")
            assert case["rollback"]["server"][table] == right[table], (name,table,"server rollback")
        assert "abort_drop_collection" in case["calls"], name


def main():
    desktop = json.loads((OUT / "desktop.json").read_text())
    native = json.loads((OUT / "native.json").read_text())
    assert [c["case"] for c in desktop] == [c["case"] for c in native] == CASES
    sd = OUT / "emulator/data/azahar-emu/sdmc/3ds/anki-3ds/phase4-v1"
    inputs = {}
    for host, console in zip(desktop, native):
        name = host["case"]
        for side,key in [("client","before"),("server","server_before")]:
            path = OUT/"fixtures"/name/f"{side}.anki2"
            inputs[f"{name}/{side}"] = hashlib.sha256(path.read_bytes()).hexdigest()
            assert snapshot(path) == host[key] == console[key], (name,"identical inputs",side)
        for side,key in [("client","after"),("server","server")]:
            assert snapshot(OUT/"desktop"/name/f"{side}.anki2") == host[key], (name,"desktop SQL evidence",side)
            assert snapshot(sd/"results"/name/f"{side}.anki2") == console[key], (name,"native SQL evidence",side)
            assert host[key] == console[key], (name,"desktop/native",side)
        assert host["calls"] == console["calls"], (name,"original server path")
        assert_oracle(host)
        assert_oracle(console)
        print(f"PASS {name}: SQL integrity, independent oracle, desktop/native equality")
    report = {"passed":len(CASES),"failed":0,"input_sha256":inputs,
              "normal_sync_trace": "emulator-report.txt", "hardware_verified": False}
    (OUT/"verification.json").write_text(json.dumps(report,indent=2)+"\n")

if __name__ == "__main__": main()
