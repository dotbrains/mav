use super::*;

#[gpui::test]
fn test_line_endings(cx: &mut gpui::App) {
    init_settings(cx, |_| {});

    cx.new(|cx| {
        let mut buffer = Buffer::local("one\r\ntwo\rthree", cx).with_language(rust_lang(), cx);
        assert_eq!(buffer.text(), "one\ntwo\nthree");
        assert_eq!(buffer.line_ending(), LineEnding::Windows);

        buffer.check_invariants();
        buffer.edit(
            [(buffer.len()..buffer.len(), "\r\nfour")],
            Some(AutoindentMode::EachLine),
            cx,
        );
        buffer.edit([(0..0, "zero\r\n")], None, cx);
        assert_eq!(buffer.text(), "zero\none\ntwo\nthree\nfour");
        assert_eq!(buffer.line_ending(), LineEnding::Windows);
        buffer.check_invariants();

        buffer
    });
}

#[gpui::test]
fn test_set_line_ending(cx: &mut TestAppContext) {
    let base = cx.new(|cx| Buffer::local("one\ntwo\nthree\n", cx));
    let base_replica = cx.new(|cx| {
        Buffer::from_proto(
            ReplicaId::new(1),
            Capability::ReadWrite,
            base.read(cx).to_proto(cx),
            None,
        )
        .unwrap()
    });
    base.update(cx, |_buffer, cx| {
        cx.subscribe(&base_replica, |this, _, event, cx| {
            if let BufferEvent::Operation {
                operation,
                is_local: true,
            } = event
            {
                this.apply_ops([operation.clone()], cx);
            }
        })
        .detach();
    });
    base_replica.update(cx, |_buffer, cx| {
        cx.subscribe(&base, |this, _, event, cx| {
            if let BufferEvent::Operation {
                operation,
                is_local: true,
            } = event
            {
                this.apply_ops([operation.clone()], cx);
            }
        })
        .detach();
    });

    // Base
    base_replica.read_with(cx, |buffer, _| {
        assert_eq!(buffer.line_ending(), LineEnding::Unix);
    });
    base.update(cx, |buffer, cx| {
        assert_eq!(buffer.line_ending(), LineEnding::Unix);
        buffer.set_line_ending(LineEnding::Windows, cx);
        assert_eq!(buffer.line_ending(), LineEnding::Windows);
    });
    base_replica.read_with(cx, |buffer, _| {
        assert_eq!(buffer.line_ending(), LineEnding::Windows);
    });
    base.update(cx, |buffer, cx| {
        buffer.set_line_ending(LineEnding::Unix, cx);
        assert_eq!(buffer.line_ending(), LineEnding::Unix);
    });
    base_replica.read_with(cx, |buffer, _| {
        assert_eq!(buffer.line_ending(), LineEnding::Unix);
    });

    // Replica
    base.read_with(cx, |buffer, _| {
        assert_eq!(buffer.line_ending(), LineEnding::Unix);
    });
    base_replica.update(cx, |buffer, cx| {
        assert_eq!(buffer.line_ending(), LineEnding::Unix);
        buffer.set_line_ending(LineEnding::Windows, cx);
        assert_eq!(buffer.line_ending(), LineEnding::Windows);
    });
    base.read_with(cx, |buffer, _| {
        assert_eq!(buffer.line_ending(), LineEnding::Windows);
    });
    base_replica.update(cx, |buffer, cx| {
        buffer.set_line_ending(LineEnding::Unix, cx);
        assert_eq!(buffer.line_ending(), LineEnding::Unix);
    });
    base.read_with(cx, |buffer, _| {
        assert_eq!(buffer.line_ending(), LineEnding::Unix);
    });
}
