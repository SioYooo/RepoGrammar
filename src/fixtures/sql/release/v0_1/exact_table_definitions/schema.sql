-- Three admitted CREATE TABLE definition lists reach the ADR-0020 support
-- threshold for SQL. Every name here is a fixture marker: none of it may reach
-- a code unit id, fact, or public surface.
CREATE TABLE fixture_accounts (
  id INTEGER PRIMARY KEY,
  email TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS fixture_orders (
  id INTEGER PRIMARY KEY,
  account_id INTEGER NOT NULL,
  placed_at TEXT
);

CREATE TABLE "FixtureItems" (
  id INTEGER PRIMARY KEY,
  order_id INTEGER NOT NULL,
  label TEXT DEFAULT 'fixture-default-label'
);
