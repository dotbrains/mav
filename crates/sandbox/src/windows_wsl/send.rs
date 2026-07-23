#[cfg(test)]
mod tests {
    use super::super::*;

    #[test]
    fn wrap_invocation_future_is_send() {
        // Callers run `wrap_invocation` via `background_spawn`, which
        // requires a `Send` future. This fails to compile if, for example, a
        // cache `MutexGuard` is ever held across an await point.
        fn assert_send<T: Send>(_: T) {}
        assert_send(wrap_invocation(
            String::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            SandboxPermissions::default(),
            None,
            HashMap::<String, String>::new(),
        ));
    }
}
