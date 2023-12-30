# `vex`

Every good PR must be correct, both in terms of functionality and style.
Whereas correct functionality can be ensured with good design and tests, as style is a more human affair, good style is far harder to pin down.
As a baseline, we should assume that code must be correctly formatted and that no lints are reported from the linter of the relevant language, however when working on large code-bases, conventions naturally start to arise and must be enforced by human reviewers.
This is where `vex` comes in.

`vex` is a highly-customisable general linter.
It takes pairs of user-provided [scheme queries][scheme] and [Lua scripts][lua], using these to scan through your source files.
When a match is found for the query, the matched syntax tree nodes are passed to the provided lua script for analysis.
Specifically, vex will use scheme queries found in `vexes/<lang>/my-lint.scm` and pass matches to `vexes/<lang>/my-lint.lua`.

[scheme]: https://tree-sitter.github.io/tree-sitter/using-parsers#pattern-matching-with-queries
[lua]: https://www.lua.org/manual/5.1/
