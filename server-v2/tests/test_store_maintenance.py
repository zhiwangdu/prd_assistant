from pathlib import Path

from logagent_v2.store import SCHEMA_VERSION, Store


def test_store_reuses_sqlite_connection_until_closed(tmp_path: Path) -> None:
    store = Store(tmp_path / "logagent.sqlite")
    store.initialize()

    with store.connect() as first:
        first.execute("SELECT 1").fetchone()
    with store.connect() as second:
        second.execute("SELECT 1").fetchone()

    assert second is first

    store.close()
    with store.connect() as reopened:
        reopened.execute("SELECT 1").fetchone()

    assert reopened is not first
    store.close()


def test_store_records_schema_version(tmp_path: Path) -> None:
    store = Store(tmp_path / "logagent.sqlite")
    store.initialize()

    with store.connect() as conn:
        user_version = conn.execute("PRAGMA user_version").fetchone()[0]
        migration = conn.execute(
            "SELECT name FROM schema_migrations WHERE version = ?",
            (SCHEMA_VERSION,),
        ).fetchone()

    assert user_version == SCHEMA_VERSION
    assert migration is not None
    assert migration["name"] == "baseline_idempotent_sqlite_schema"
    store.close()
