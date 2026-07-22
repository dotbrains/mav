use super::*;

// always_confirm patterns on non-terminal tools
#[test]
fn always_confirm_works_for_file_tools() {
    t("sensitive.env")
        .tool(EditFileTool::NAME)
        .confirm(&["sensitive"])
        .is_confirm();

    t("normal.txt")
        .tool(EditFileTool::NAME)
        .confirm(&["sensitive"])
        .mode(ToolPermissionMode::Allow)
        .is_allow();

    t("/etc/config")
        .tool(DeletePathTool::NAME)
        .confirm(&["/etc/"])
        .is_confirm();

    t("/home/user/safe.txt")
        .tool(DeletePathTool::NAME)
        .confirm(&["/etc/"])
        .mode(ToolPermissionMode::Allow)
        .is_allow();

    t("https://secret.internal.com/api")
        .tool(FetchTool::NAME)
        .confirm(&["secret\\.internal"])
        .is_confirm();

    t("https://public.example.com/api")
        .tool(FetchTool::NAME)
        .confirm(&["secret\\.internal"])
        .mode(ToolPermissionMode::Allow)
        .is_allow();

    // confirm on non-terminal tools still beats allow
    t("sensitive.env")
        .tool(EditFileTool::NAME)
        .allow(&["sensitive"])
        .confirm(&["\\.env$"])
        .is_confirm();

    // confirm on non-terminal tools is still beaten by deny
    t("sensitive.env")
        .tool(EditFileTool::NAME)
        .confirm(&["sensitive"])
        .deny(&["\\.env$"])
        .is_deny();

    // global default allow does not bypass confirm on non-terminal tools
    t("/etc/passwd")
        .tool(EditFileTool::NAME)
        .confirm(&["/etc/"])
        .global_default(ToolPermissionMode::Allow)
        .is_confirm();
}
