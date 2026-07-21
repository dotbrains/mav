    #[test]
    fn test_parse_core_worktree_accepts_simple_and_quoted_values() {
        assert_eq!(
            parse_core_worktree("[core]\n\tworktree = ../../../sub\n"),
            Some("../../../sub".to_string())
        );
        assert_eq!(
            parse_core_worktree("[core]\n\tworktree = \"../../../sub with spaces\"\n"),
            Some("../../../sub with spaces".to_string())
        );
        assert_eq!(
            parse_core_worktree("[core]\n\tworktree = \"C:/Users/Test/project/sub\"\n"),
            Some("C:/Users/Test/project/sub".to_string())
        );
        assert_eq!(
            parse_core_worktree("[core]\n\tworktree = \"C:\\\\Users\\\\Test\\\\project\\\\sub\"\n"),
            Some("C:\\Users\\Test\\project\\sub".to_string())
        );
    }

    #[test]
    fn test_parse_core_worktree_rejects_ambiguous_or_unsupported_config() {
        assert_eq!(parse_core_worktree("[core]\n\tworktree =\n"), None);
        assert_eq!(
            parse_core_worktree("[core]\n\tworktree = ../../../sub\n\tworktree = ../../../other\n"),
            None
        );
        assert_eq!(parse_core_worktree("worktree = ../../../sub\n"), None);
        assert_eq!(
            parse_core_worktree(
                "[include]\n\tpath = ../config\n[core]\n\tworktree = ../../../sub\n"
            ),
            None
        );
        assert_eq!(
            parse_core_worktree("[core]\n\tworktree = \"../../../sub\" trailing\n"),
            None
        );
        assert_eq!(
            parse_core_worktree("[core]\n\tworktree = \"../../../sub\n"),
            None
        );
        assert_eq!(
            parse_core_worktree("[core]\n\tworktree = ../../../sub\\\n"),
            None
        );
        assert_eq!(parse_core_worktree("[core]\n\tworktree = ~/sub\n"), None);
    }
