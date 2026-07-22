    cx.dispatch_action(SelectFirst);
    assert_entries_after!(
        cx,
        running_state,
        SelectFirst,
        [
            "v Scope 1 <=== selected",
            "    > variable1",
            "    > variable2",
            "> Scope 2",
        ]
    );

    assert_entries_after!(
        cx,
        running_state,
        SelectNext,
        [
            "v Scope 1",
            "    > variable1 <=== selected",
            "    > variable2",
            "> Scope 2",
        ]
    );

    // expand the nested variables of variable 1
    assert_entries_after!(
        cx,
        running_state,
        ExpandSelectedEntry,
        [
            "v Scope 1",
            "    v variable1 <=== selected",
            "        > nested1",
            "        > nested2",
            "    > variable2",
            "> Scope 2",
        ]
    );

    // select the first nested variable of variable 1
    assert_entries_after!(
        cx,
        running_state,
        SelectNext,
        [
            "v Scope 1",
            "    v variable1",
            "        > nested1 <=== selected",
            "        > nested2",
            "    > variable2",
            "> Scope 2",
        ]
    );

    // select the second nested variable of variable 1
    assert_entries_after!(
        cx,
        running_state,
        SelectNext,
        [
            "v Scope 1",
            "    v variable1",
            "        > nested1",
            "        > nested2 <=== selected",
            "    > variable2",
            "> Scope 2",
        ]
    );

    // select variable 2 of scope 1
    assert_entries_after!(
        cx,
        running_state,
        SelectNext,
        [
            "v Scope 1",
            "    v variable1",
            "        > nested1",
            "        > nested2",
            "    > variable2 <=== selected",
            "> Scope 2",
        ]
    );

    // select scope 2
    assert_entries_after!(
        cx,
        running_state,
        SelectNext,
        [
            "v Scope 1",
            "    v variable1",
            "        > nested1",
            "        > nested2",
            "    > variable2",
            "> Scope 2 <=== selected",
        ]
    );

    // expand the nested variables of scope 2
    assert_entries_after!(
        cx,
        running_state,
        ExpandSelectedEntry,
        [
            "v Scope 1",
            "    v variable1",
            "        > nested1",
            "        > nested2",
            "    > variable2",
            "v Scope 2 <=== selected",
            "    > variable3",
        ]
    );

    // select variable 3 of scope 2
    assert_entries_after!(
        cx,
        running_state,
        SelectNext,
        [
            "v Scope 1",
            "    v variable1",
            "        > nested1",
            "        > nested2",
            "    > variable2",
            "v Scope 2",
            "    > variable3 <=== selected",
        ]
    );

    // select scope 2
    assert_entries_after!(
        cx,
        running_state,
        SelectPrevious,
        [
            "v Scope 1",
            "    v variable1",
            "        > nested1",
            "        > nested2",
            "    > variable2",
            "v Scope 2 <=== selected",
            "    > variable3",
        ]
    );

    // collapse variables of scope 2
    assert_entries_after!(
        cx,
        running_state,
        CollapseSelectedEntry,
        [
            "v Scope 1",
            "    v variable1",
            "        > nested1",
            "        > nested2",
            "    > variable2",
            "> Scope 2 <=== selected",
        ]
    );

    // select variable 2 of scope 1
    assert_entries_after!(
        cx,
        running_state,
        SelectPrevious,
        [
            "v Scope 1",
            "    v variable1",
            "        > nested1",
            "        > nested2",
            "    > variable2 <=== selected",
            "> Scope 2",
        ]
    );

    // select nested2 of variable 1
    assert_entries_after!(
        cx,
        running_state,
        SelectPrevious,
        [
            "v Scope 1",
            "    v variable1",
            "        > nested1",
            "        > nested2 <=== selected",
            "    > variable2",
            "> Scope 2",
        ]
    );

    // select nested1 of variable 1
    assert_entries_after!(
        cx,
        running_state,
        SelectPrevious,
        [
            "v Scope 1",
            "    v variable1",
            "        > nested1 <=== selected",
            "        > nested2",
            "    > variable2",
            "> Scope 2",
        ]
    );

    // select variable 1 of scope 1
    assert_entries_after!(
        cx,
        running_state,
        SelectPrevious,
        [
            "v Scope 1",
            "    v variable1 <=== selected",
            "        > nested1",
            "        > nested2",
            "    > variable2",
            "> Scope 2",
        ]
    );

    // collapse variables of variable 1
    assert_entries_after!(
        cx,
        running_state,
        CollapseSelectedEntry,
        [
            "v Scope 1",
            "    > variable1 <=== selected",
            "    > variable2",
            "> Scope 2",
        ]
    );

    // select scope 1
    assert_entries_after!(
        cx,
        running_state,
        SelectPrevious,
        [
            "v Scope 1 <=== selected",
            "    > variable1",
            "    > variable2",
            "> Scope 2",
        ]
    );

    // collapse variables of scope 1
    assert_entries_after!(
        cx,
        running_state,
        CollapseSelectedEntry,
        ["> Scope 1 <=== selected", "> Scope 2"]
    );

    // select scope 2 backwards
    assert_entries_after!(
        cx,
        running_state,
        SelectPrevious,
        ["> Scope 1", "> Scope 2 <=== selected"]
    );

    // select scope 1 backwards
    assert_entries_after!(
        cx,
        running_state,
        SelectNext,
        ["> Scope 1 <=== selected", "> Scope 2"]
    );

    // test stepping through nested with ExpandSelectedEntry/CollapseSelectedEntry actions

    assert_entries_after!(
        cx,
        running_state,
        ExpandSelectedEntry,
        [
            "v Scope 1 <=== selected",
            "    > variable1",
            "    > variable2",
            "> Scope 2",
        ]
    );

    assert_entries_after!(
        cx,
        running_state,
        ExpandSelectedEntry,
        [
            "v Scope 1",
            "    > variable1 <=== selected",
            "    > variable2",
            "> Scope 2",
        ]
    );

    assert_entries_after!(
        cx,
        running_state,
        ExpandSelectedEntry,
        [
            "v Scope 1",
            "    v variable1 <=== selected",
            "        > nested1",
            "        > nested2",
            "    > variable2",
            "> Scope 2",
        ]
    );

    assert_entries_after!(
        cx,
        running_state,
        ExpandSelectedEntry,
        [
            "v Scope 1",
            "    v variable1",
            "        > nested1 <=== selected",
            "        > nested2",
            "    > variable2",
            "> Scope 2",
        ]
    );

    assert_entries_after!(
        cx,
        running_state,
        ExpandSelectedEntry,
        [
            "v Scope 1",
            "    v variable1",
            "        > nested1",
            "        > nested2 <=== selected",
            "    > variable2",
            "> Scope 2",
        ]
    );

    assert_entries_after!(
        cx,
        running_state,
        ExpandSelectedEntry,
        [
            "v Scope 1",
            "    v variable1",
            "        > nested1",
            "        > nested2",
            "    > variable2 <=== selected",
            "> Scope 2",
        ]
    );

    assert_entries_after!(
        cx,
        running_state,
        CollapseSelectedEntry,
        [
            "v Scope 1",
            "    v variable1",
            "        > nested1",
            "        > nested2 <=== selected",
            "    > variable2",
            "> Scope 2",
        ]
    );

    assert_entries_after!(
        cx,
        running_state,
        CollapseSelectedEntry,
        [
            "v Scope 1",
            "    v variable1",
            "        > nested1 <=== selected",
            "        > nested2",
            "    > variable2",
            "> Scope 2",
        ]
    );

    assert_entries_after!(
        cx,
        running_state,
        CollapseSelectedEntry,
        [
            "v Scope 1",
            "    v variable1 <=== selected",
            "        > nested1",
            "        > nested2",
            "    > variable2",
            "> Scope 2",
        ]
    );

    assert_entries_after!(
        cx,
        running_state,
        CollapseSelectedEntry,
        [
            "v Scope 1",
            "    > variable1 <=== selected",
            "    > variable2",
            "> Scope 2",
        ]
    );

    assert_entries_after!(
        cx,
        running_state,
        CollapseSelectedEntry,
        [
            "v Scope 1 <=== selected",
            "    > variable1",
            "    > variable2",
            "> Scope 2",
        ]
    );

    assert_entries_after!(
        cx,
        running_state,
        CollapseSelectedEntry,
        ["> Scope 1 <=== selected", "> Scope 2"]
    );
