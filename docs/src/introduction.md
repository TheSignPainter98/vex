# Introduction

Vex is an enforcer of local style guidelines, in the form of a hackable linter.

Vex scans source files for non-idiomatic code-patterns expressed via a set of user-provided [Starlark][starlark] scripts.
These scripts declare [tree-sitter queries][tree-sitter-query-syntax], reason about their results and create pretty, human-readable warnings which annotate source files.

When working on any codebase---especially large ones---conventions naturally arise and although ultimately noble in intent, they all share a significant problem.
Typical tools for code standardisation express preferences at the level of _all code_ written in a particular language but conventions are most often at the level of _specific code_ and hence no pattern gets enforced.
Therefore, to ensure consistency, we must rely on fallible human review to identify and help eliminate non-standard code-patterns.
Consistency is now at the mercy of human mistakes, hence inconsistency is inevitable.
This need not be the way.

Vex allows subjective style preferences to be defined at a level closer to the project, specifically through a folder of vexes in the project root.
These are then mechanically enforced throughout the project, leaving no room for later ugly surprises.
For truly excellent code, Vex is used alongside language-specific formatters and linters.

This tool is perfect for anyone looking to improve the consistency and hence the quality of their or their team’s code.
To get started, [install Vex](./installation.md) and then follow the [let’s write a vex tutorial](./tutorials/lets-write-a-vex.md).
The rest of these docs is organised as follows:
<div class="quote-grid">
    <blockquote>
            <p>
                <div class="diataxis-card-header"><a href="./tutorials/index.html">Tutorials</a></div>
                Hands-on lessons in vex-writing
            </p>
        </a>
    </blockquote>
    <blockquote>
        <p>
            <div class="diataxis-card-header">
                <a href="./how-to-guides/index.html">How-to guides</a>
            </div>
            Step-by-step instructions for common tasks
        </p>
    </blockquote>
    <blockquote>
        <p>
            <div class="diataxis-card-header">
                <a href="./reference-materials/index.html">Reference materials</a>
            </div>
            Technical information about vex
        </p>
    </blockquote>
    <blockquote>
        <p>
            <div class="diataxis-card-header">
                <a href="./explanations/index.html">Explanations</a>
            </div>
            Long-form discussion of key-topics
        </p>
    </blockquote>
</div>

[tree-sitter-query-syntax]: https://tree-sitter.github.io/tree-sitter/using-parsers#query-syntax
[starlark]: https://github.com/bazelbuild/starlark/
