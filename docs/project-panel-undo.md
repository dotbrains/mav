# Project Panel Undo

The project panel undo manager records semantic filesystem changes and applies
their inverse for undo and redo.

## Operations and Results

Undo and redo actions execute an operation against the filesystem, producing a
result that is recorded back into the history in place of the original entry.
Each result is the semantic inverse of its paired operation, so the cycle can
repeat for continued undo and redo.

```text
Operations                         Results
---------------------------------  --------------------------------------
Create(ProjectPath)                Created(ProjectPath)
Trash(ProjectPath)                 Trashed(TrashedEntry)
Rename(ProjectPath, ProjectPath)   Renamed(ProjectPath, ProjectPath)
Restore(TrashedEntry)              Restored(ProjectPath)
Batch(Vec<Operation>)              Batch(Vec<Result>)
```

## History and Cursor

The undo manager maintains an operation history with a cursor position. Recording
an operation appends it to the history and advances the cursor to the end. The
cursor separates past entries from future entries.

Undo takes the history entry just before the cursor, executes its inverse,
records the result back in its place, and moves the cursor one step left.

Redo takes the history entry at the cursor, executes its inverse, records the
result back in its place, and advances the cursor one step right.

## Example

```text
User Operation  Create(src/main.rs)
History
    0 Created(src/main.rs)
    1 +++cursor+++

User Operation  Rename(README.md, readme.md)
History
    0 Created(src/main.rs)
    1 Renamed(README.md, readme.md)
    2 +++cursor+++

User Operation  Create(CONTRIBUTING.md)
History
    0 Created(src/main.rs)
    1 Renamed(README.md, readme.md)
    2 Created(CONTRIBUTING.md)
    3 +++cursor+++

User Operation  Undo
Execute         Created(CONTRIBUTING.md) -> Trash(CONTRIBUTING.md)
Record          Trashed(TrashedEntry(1))
History
    0 Created(src/main.rs)
    1 Renamed(README.md, readme.md)
    2 +++cursor+++
    2 Trashed(TrashedEntry(1))

User Operation  Undo
Execute         Renamed(README.md, readme.md) -> Rename(readme.md, README.md)
Record          Renamed(readme.md, README.md)
History
    0 Created(src/main.rs)
    1 +++cursor+++
    1 Renamed(readme.md, README.md)
    2 Trashed(TrashedEntry(1))

User Operation  Redo
Execute         Renamed(readme.md, README.md) -> Rename(README.md, readme.md)
Record          Renamed(README.md, readme.md)
History
    0 Created(src/main.rs)
    1 Renamed(README.md, readme.md)
    2 +++cursor+++
    2 Trashed(TrashedEntry(1))

User Operation  Redo
Execute         Trashed(TrashedEntry(1)) -> Restore(TrashedEntry(1))
Record          Restored(ProjectPath)
History
    0 Created(src/main.rs)
    1 Renamed(README.md, readme.md)
    2 Restored(ProjectPath)
    3 +++cursor+++
```

## Concurrency Notes

Undo and redo operate on the filesystem asynchronously. If a user performs
another filesystem operation while an undo is still in flight, the result may no
longer match the original history entry. The implementation treats failures as
terminal for that entry: it removes the failing change from history so the cursor
does not point outside the remaining history.

The long-term direction is to track tainted files that should not be touched by
new user operations while an inverse operation is pending.
