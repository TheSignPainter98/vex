# Introduction

Vex is an enforcer of local style guidelines, in the form of a hackable linter.

Vex scans source files for non-idiomatic code-patterns expressed in a set of user-provided [Starlark][starlark] scripts.
These scripts declare [tree-sitter queries][tree-sitter-query-syntax], reason about their results and create pretty, human-readable warnings which annotate source files.

When working on any codebase---especially large ones---conventions naturally arise and although ultimately noble in intent, they all have a significant problem.
The commonmost tools for code standardisation express preferences at the level of _all code_ written in a particular language but our preferences are at the level of the _specific code_ we have in front of us.
It is simply not possible for a centralised tool to express good opinions in every case, hence we rely must rely on human review to identify and help eliminate non-standard code-patterns.
To err is to be human and it would seem hence that to be human is to have inconsistent code, but this need not be the case.

Vex allows subjective style preferences to be encoded at a level closer to projects.
By placing a folder of vexes in the root of a project, we may express _project-local_ preferences.
Furthermore, if that folder contains a shared repository of vexes, we may express _organisation-local_ preferences.
By employing vex, language-specific linters and formatters, clean code is within reach.

Exactly which preferences are to be enforced is up to you, these docs will show you not only how to [install Vex](./installation.md) but also its ins and outs, organised as follows:

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
