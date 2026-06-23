# Todo CLI — Product Specification

## Goal

We are building a command-line interface application called `todo`. It allows the
user to add new tasks, list all of their existing tasks, and mark a task as done.
The application must persist all of the task data to a JSON file on disk, so that
the user's tasks are remembered between separate runs of the program.

## Constraints

The application must be written in TypeScript and must run on the Bun runtime.
All of the task data should be stored in a single JSON file, located at the path
`~/.todo.json` in the user's home directory. We are explicitly choosing not to
use a database of any kind for this project. The application must also not make
any network connections whatsoever; everything it does should happen locally.

## Interfaces

The application exposes the following commands to the outside world:

- Running `todo add <text>` should append a new task containing the given text
  to the list of tasks, and then print the new task's identifier to standard out.
- Running `todo ls` should print a table of all tasks to standard out, showing
  each task's identifier, its text, and whether or not it has been completed.
- Running `todo done <id>` should find the task with the given identifier and
  mark it as done.

The data file at `~/.todo.json` contains a JSON array of objects, where each
object has an `id`, a `text`, and a `done` field.

## Invariants

These are the properties that must always hold true:

1. Every time the application writes to the data file, that write must be atomic.
   This should be achieved by first writing the new contents to a temporary file
   and then renaming that temporary file into place, so a crash mid-write can
   never leave the data file in a corrupt, partially written state.
2. Task identifiers must be monotonically increasing, and an identifier must
   never be reused, even after the task it belonged to has been deleted.
3. If the user runs `todo done <id>` with an identifier that does not exist, the
   application must exit with a non-zero status code and print a helpful error
   message to standard error.

## Tasks

The work is broken down into the following tasks:

1. Scaffold the Bun command-line application. (Done)
2. Implement the `add` and `ls` commands. (Done)
3. Implement the `done` command, including the guard for unknown identifiers
   described in invariant 3. (In progress)
4. Implement the atomic write described in invariant 1. (Not started)

## Known Bugs

- On 2026-06-20 we found that a crash during a write could leave the JSON data
  file partially written and therefore corrupt. This is addressed by invariant 1.
