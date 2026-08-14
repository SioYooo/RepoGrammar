-- Three otherwise-admitted definitions precede a construct PostgreSQL and
-- SQLite lex differently. Because the token stream forks there, no statement
-- boundary in this file is proven and the whole file must yield no anchor.
CREATE TABLE fixture_diverged_a (id INTEGER PRIMARY KEY);
CREATE TABLE fixture_diverged_b (id INTEGER PRIMARY KEY);
CREATE TABLE fixture_diverged_c (id INTEGER PRIMARY KEY);

CREATE FUNCTION fixture_diverged_fn() RETURNS INTEGER AS $$ SELECT 1 $$;
