# Schema

Corrosion's schema definition happens via files each representing one or more tables and views, written in SQLite-flavored SQL. This is done through `CREATE TABLE`, `CREATE INDEX`, and `CREATE VIEW`.

Manual migrations are not supported (yet). When schema files change, Corrosion can be reloaded (or restarted) and it will compute a diff between the old and new schema and make the changes.

Any destructive actions on the table schemas are ignored / prohibited. This includes removing a table definition entirely or removing a column from a table. Indexes can be removed or added.

## Constraints

- Only `CREATE TABLE`, `CREATE INDEX`, `CREATE VIEW`, and `DROP VIEW IF EXISTS` are allowed
- No unique indexes allowed (except for the default primary key unique index that does not need to be created)
- The primary key must be non nullable
- Non-nullable columns require a default value
  - This is a cr-sqlite constraint, but in practice w/ Corrosion: it does not matter. Entire changes will be applied all at once and no fields will be missing.
  - If table schemas are modified, then a default value is definitely required.

## Example

```sql
-- /etc/corrosion/schema/apps.sql

CREATE TABLE apps (
    id INT NOT NULL PRIMARY KEY,
    name TEXT NOT NULL DEFAULT "",
    user_id INT NOT NULL DEFAULT 0
);

CREATE INDEX apps_user_id ON apps (user_id);

CREATE VIEW named_apps AS
SELECT id, name
FROM apps
WHERE name != '';
```

Views are ordinary SQLite views that exist locally on every node which loads
the managed schema. A view is not registered as a CR-SQLite CRR and does not
replicate data or create CR-SQLite metadata. Instead, its underlying tables
replicate normally and each node evaluates the view against its local data.

`CREATE VIEW IF NOT EXISTS` is accepted, but managed view creation is still
strict: an existing conflicting view is not silently retained. To remove a
managed view, use `DROP VIEW IF EXISTS view_name`; plain `DROP VIEW` is not
supported. Temporary views are not supported.
