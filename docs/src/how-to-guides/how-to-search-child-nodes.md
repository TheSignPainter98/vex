# How to search child nodes

This guide provides two solutions.
If only one matching child is needed, use the first.
If all matching children is needed, use the second.

## How to search child nodes, returning the first match

1. Put the cursor at the end of vex script.
2. If absent, copy the following function into the script—
    ```python
    def find_child(node, predicate):
        for child in node.children():
            if predicate(child):
                return child

            match = find_child(child, predicate)
            if match != None:
                return match
        return None
    ```
3. Move to where the matching child is required.
4. Call the `find_child` function, passing the node whose children are to be searched and a function or lambda which takes a node returns whether it is the desired one.
    For example, to find the first child `let_declaration` node, use the following—
    ```python
     find_child(
        node_to_search,
        lambda node: node.kind == 'let_declaration',
    )
    ```

## How to search child nodes, returning all matches

1. Put the cursor at the end of vex script.
2. If absent, copy the following function into the script—
    ```python
    def find_all_children(node, predicate, allow_nested=False):
        def _find_children(node, predicate, allow_nested, ret)
            for child in node.children():
                if predicate(child):
                    ret.append(child)
                    if not allow_nested:
                        continue

                _find_children(child, predicate, allow_nested=allow_nested, ret=ret)

        ret = []
        _find_children(node, predicate, allow_nested, ret)
        return ret
    ```
3. Move to where the matching child is required.
4. Call the `find_all_children` function, passing the node whose children are to be searched and a function or lambda which takes a node returns whether to include it in the output.
    For example, to find all child `let_declaration` nodes, use the following—
    ```python
     find_all_children(
        node_to_search,
        lambda node: node.kind == 'let_declaration',
        allow_nested=True,
    )
    ```
