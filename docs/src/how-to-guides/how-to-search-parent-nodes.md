# How to search parent nodes

This guide provides two solutions.
If only one matching parent is needed, use the first.
If all matching parents is needed, use the second.

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
3. Move to where the matching child is required.
4. Call the `find_parent` function, passing the node whose parents are to be searched and a function or lambda which takes a node returns whether it is the desired one.
    For example, to find the first parent `let_declaration` node, use the following—
    ```python
    find_parent(
        node_to_search,
        lambda node: node.kind == 'let_declaration',
    )
    ```

## How to search parent nodes, returning all matches

1. Put the cursor at the end of vex script.
2. If absent, copy the following function into the script—
    ```python
    def find_all_parents(node, predicate, allow_nested=False):
        return [ parent for parent in node.parents() if predicate(parent) ]
    ```
3. Move to where the matching child is required.
4. Call the `find_all_parents` function, passing the node whose parents are to be searched and a function or lambda which takes a node returns whether to include it in the output.
    For example, to find all parent `let_declaration` nodes, use the following—
    ```python
    find_all_parents(
        node_to_search,
        lambda node: node.kind == 'let_declaration',
    )
    ```
    The variable `let_declarations` is now a list.
