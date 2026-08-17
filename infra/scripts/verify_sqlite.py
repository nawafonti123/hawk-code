"""Read-only verification helper for a HAWK Code SQLite database."""

from __future__ import annotations

import argparse
import sqlite3
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("database", type=Path)
    args = parser.parse_args()

    database = args.database.resolve(strict=True)
    connection = sqlite3.connect(f"file:{database.as_posix()}?mode=ro", uri=True)
    tables = [
        row[0]
        for row in connection.execute(
            "SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name"
        )
    ]
    migrations = list(
        connection.execute(
            "SELECT version, description, success "
            "FROM _sqlx_migrations ORDER BY version"
        )
    )

    print({"tables": tables, "migrations": migrations})


if __name__ == "__main__":
    main()
