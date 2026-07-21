use super::*;

#[gpui::test]
fn test_companion_spacer_blocks(cx: &mut gpui::TestAppContext) {
    cx.update(init_test);

    let base_text = "aaa\nbbb\nccc\nddd\nddd\nddd\neee\n";
    let main_text = "aaa\nddd\nddd\nddd\nXXX\nYYY\nZZZ\neee\n";

    let rhs_buffer = cx.new(|cx| Buffer::local(main_text, cx));
    let diff = cx.new(|cx| {
        BufferDiff::new_with_base_text(base_text, &rhs_buffer.read(cx).text_snapshot(), cx)
    });
    let lhs_buffer = diff.read_with(cx, |diff, _| diff.base_text_buffer().clone());

    let lhs_multibuffer = cx.new(|cx| {
        let mut mb = MultiBuffer::new(Capability::ReadWrite);
        mb.set_excerpts_for_buffer(
            lhs_buffer.clone(),
            [Point::zero()..lhs_buffer.read(cx).max_point()],
            0,
            cx,
        );
        mb.add_inverted_diff(diff.clone(), rhs_buffer.clone(), cx);
        mb
    });
    let rhs_multibuffer = cx.new(|cx| {
        let mut mb = MultiBuffer::new(Capability::ReadWrite);
        mb.set_excerpts_for_buffer(
            rhs_buffer.clone(),
            [Point::zero()..rhs_buffer.read(cx).max_point()],
            0,
            cx,
        );
        mb.add_diff(diff.clone(), cx);
        mb
    });
    let subscription = rhs_multibuffer.update(cx, |rhs_multibuffer, _| rhs_multibuffer.subscribe());

    let lhs_buffer_snapshot = cx.update(|cx| lhs_multibuffer.read(cx).snapshot(cx));
    let (mut _lhs_inlay_map, lhs_inlay_snapshot) = InlayMap::new(lhs_buffer_snapshot);
    let (mut _lhs_fold_map, lhs_fold_snapshot) = FoldMap::new(lhs_inlay_snapshot);
    let (mut _lhs_tab_map, lhs_tab_snapshot) =
        TabMap::new(lhs_fold_snapshot, 4.try_into().unwrap());
    let (_lhs_wrap_map, lhs_wrap_snapshot) =
        cx.update(|cx| WrapMap::new(lhs_tab_snapshot, font("Helvetica"), px(14.0), None, cx));
    let lhs_block_map = BlockMap::new(lhs_wrap_snapshot.clone(), 0, 0);

    let rhs_buffer_snapshot = cx.update(|cx| rhs_multibuffer.read(cx).snapshot(cx));
    let (mut rhs_inlay_map, rhs_inlay_snapshot) = InlayMap::new(rhs_buffer_snapshot);
    let (mut rhs_fold_map, rhs_fold_snapshot) = FoldMap::new(rhs_inlay_snapshot);
    let (mut rhs_tab_map, rhs_tab_snapshot) = TabMap::new(rhs_fold_snapshot, 4.try_into().unwrap());
    let (_rhs_wrap_map, rhs_wrap_snapshot) =
        cx.update(|cx| WrapMap::new(rhs_tab_snapshot, font("Helvetica"), px(14.0), None, cx));
    let rhs_block_map = BlockMap::new(rhs_wrap_snapshot.clone(), 0, 0);

    let rhs_entity_id = rhs_multibuffer.entity_id();

    let companion = cx.new(|_| Companion::new(rhs_entity_id));

    let rhs_edits = Patch::new(vec![text::Edit {
        old: WrapRow(0)..rhs_wrap_snapshot.max_point().row(),
        new: WrapRow(0)..rhs_wrap_snapshot.max_point().row(),
    }]);
    let lhs_edits = Patch::new(vec![text::Edit {
        old: WrapRow(0)..lhs_wrap_snapshot.max_point().row(),
        new: WrapRow(0)..lhs_wrap_snapshot.max_point().row(),
    }]);

    let rhs_snapshot = companion.read_with(cx, |companion, _cx| {
        rhs_block_map.read(
            rhs_wrap_snapshot.clone(),
            rhs_edits.clone(),
            Some(CompanionView::new(
                rhs_entity_id,
                &lhs_wrap_snapshot,
                &lhs_edits,
                companion,
            )),
        )
    });

    let lhs_entity_id = lhs_multibuffer.entity_id();
    let lhs_snapshot = companion.read_with(cx, |companion, _cx| {
        lhs_block_map.read(
            lhs_wrap_snapshot.clone(),
            lhs_edits.clone(),
            Some(CompanionView::new(
                lhs_entity_id,
                &rhs_wrap_snapshot,
                &rhs_edits,
                companion,
            )),
        )
    });

    // LHS:
    //   aaa
    // - bbb
    // - ccc
    //   ddd
    //   ddd
    //   ddd
    //   <extra line>
    //   <extra line>
    //   <extra line>
    //   *eee
    //
    // RHS:
    //   aaa
    //   <extra line>
    //   <extra line>
    //   ddd
    //   ddd
    //   ddd
    // + XXX
    // + YYY
    // + ZZZ
    //   eee

    assert_eq!(
        rhs_snapshot.snapshot.text(),
        "aaa\n\n\nddd\nddd\nddd\nXXX\nYYY\nZZZ\neee\n",
        "RHS should have 2 spacer lines after 'aaa' to align with LHS's deleted lines"
    );

    assert_eq!(
        lhs_snapshot.snapshot.text(),
        "aaa\nbbb\nccc\nddd\nddd\nddd\n\n\n\neee\n",
        "LHS should have 3 spacer lines in place of RHS's inserted lines"
    );

    // LHS:
    //   aaa
    // - bbb
    // - ccc
    //   ddd
    //   ddd
    //   ddd
    //   <extra line>
    //   <extra line>
    //   <extra line>
    //   eee
    //
    // RHS:
    //   aaa
    //   <extra line>
    //   <extra line>
    //   ddd
    //   foo
    //   foo
    //   foo
    //   ddd
    //   ddd
    // + XXX
    // + YYY
    // + ZZZ
    //   eee

    let rhs_buffer_snapshot = rhs_multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.edit(
            [(Point::new(2, 0)..Point::new(2, 0), "foo\nfoo\nfoo\n")],
            None,
            cx,
        );
        multibuffer.snapshot(cx)
    });

    let (rhs_inlay_snapshot, rhs_inlay_edits) =
        rhs_inlay_map.sync(rhs_buffer_snapshot, subscription.consume().into_inner());
    let (rhs_fold_snapshot, rhs_fold_edits) =
        rhs_fold_map.read(rhs_inlay_snapshot, rhs_inlay_edits);
    let (rhs_tab_snapshot, rhs_tab_edits) =
        rhs_tab_map.sync(rhs_fold_snapshot, rhs_fold_edits, 4.try_into().unwrap());
    let (rhs_wrap_snapshot, rhs_wrap_edits) = _rhs_wrap_map.update(cx, |wrap_map, cx| {
        wrap_map.sync(rhs_tab_snapshot, rhs_tab_edits, cx)
    });

    let rhs_snapshot = companion.read_with(cx, |companion, _cx| {
        rhs_block_map.read(
            rhs_wrap_snapshot.clone(),
            rhs_wrap_edits.clone(),
            Some(CompanionView::new(
                rhs_entity_id,
                &lhs_wrap_snapshot,
                &Default::default(),
                companion,
            )),
        )
    });

    let lhs_snapshot = companion.read_with(cx, |companion, _cx| {
        lhs_block_map.read(
            lhs_wrap_snapshot.clone(),
            Default::default(),
            Some(CompanionView::new(
                lhs_entity_id,
                &rhs_wrap_snapshot,
                &rhs_wrap_edits,
                companion,
            )),
        )
    });

    assert_eq!(
        rhs_snapshot.snapshot.text(),
        "aaa\n\n\nddd\nfoo\nfoo\nfoo\nddd\nddd\nXXX\nYYY\nZZZ\neee\n",
        "RHS should have the insertion"
    );

    assert_eq!(
        lhs_snapshot.snapshot.text(),
        "aaa\nbbb\nccc\nddd\n\n\n\nddd\nddd\n\n\n\neee\n",
        "LHS should have 3 more spacer lines to balance the insertion"
    );
}
