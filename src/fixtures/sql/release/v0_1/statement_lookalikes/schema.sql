-- Nothing here is an admitted CREATE TABLE, though every line reads like one.
-- CREATE TABLE fixture_ghost_one (id INTEGER);
/* CREATE TABLE fixture_ghost_two (id INTEGER); */
INSERT INTO fixture_log (body)
  VALUES ('CREATE TABLE fixture_ghost_three (id INTEGER);');
CREATE TEMP TABLE fixture_ghost_four (id INTEGER);
CREATE TABLE fixture_ghost_five AS SELECT 1;
CREATE VIEW fixture_ghost_six AS SELECT 1;
CREATE INDEX fixture_ghost_seven ON fixture_log (body);
