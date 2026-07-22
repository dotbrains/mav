pub(super) fn floor_char_boundary(s: &str, index: usize) -> usize {
    if index >= s.len() {
        s.len()
    } else if s.is_char_boundary(index) {
        index
    } else {
        // Find the nearest valid character boundary at or before index
        (0..index)
            .rev()
            .find(|&i| s.is_char_boundary(i))
            .unwrap_or(0)
    }
}
