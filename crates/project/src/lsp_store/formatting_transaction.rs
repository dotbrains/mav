use super::*;

/// Apply edits to the buffer that will become part of the formatting transaction.
/// Fails if the buffer has been edited since the start of that transaction.
pub(super) fn extend_formatting_transaction(
    buffer: &FormattableBuffer,
    formatting_transaction_id: text::TransactionId,
    cx: &mut AsyncApp,
    operation: impl FnOnce(&mut Buffer, &mut Context<Buffer>),
) -> anyhow::Result<()> {
    buffer.handle.update(cx, |buffer, cx| {
        let last_transaction_id = buffer.peek_undo_stack().map(|t| t.transaction_id());
        if last_transaction_id != Some(formatting_transaction_id) {
            anyhow::bail!("Buffer edited while formatting. Aborting")
        }
        buffer.start_transaction();
        operation(buffer, cx);
        if let Some(transaction_id) = buffer.end_transaction(cx) {
            buffer.merge_transactions(transaction_id, formatting_transaction_id);
        }
        Ok(())
    })
}
