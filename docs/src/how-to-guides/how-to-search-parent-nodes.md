# How to search parent nodes

The approach to take is slightly different depending on whether a single matching parent is required, or all matching parents are required.

## How to search parent nodes, returning the first match

1. Put the cursor at the end of vex script.
2. If absent, copy the following function into the script—
    ```python
    def find_parent(node, predicate):
        for parent in node.parents():
            if predicate(parent):
                return parent
        return None
    ```
3. Call the `find_parent` function, passing the node whose parents are to be searched and a function or lambda which takes a node returns whether it is the desired one—
    ```python
    let_declaration = find_parent(
        node_to_search,
        lambda node: node.kind == 'let_declaration',
    )
    ```
    The variable `let_declaration` is now either a `Node` or `None`.

## How to search parent nodes, returning all matches

1. Put the cursor at the end of vex script.
2. If absent, copy the following function into the script—
    ```python
    def find_parents(node, predicate, allow_nested=False):
        return [ parent for parent in node.parents() if predicate(parent) ]
    ```
3. Call the `find_parents` function, passing the node whose parents are to be searched and a function or lambda which takes a node returns whether to include it in the output, e.g.—
    ```python
    let_declarations = find_parents(
        node_to_search,
        lambda node: node.kind == 'let_declaration',
    )
    ```
    The variable `let_declarations` is now a list.
