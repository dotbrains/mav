use super::*;

/// Parse an order specification text into groups of edit indexes.
/// Supports numbers, ranges (a-b), commas, comments starting with `//`, and blank lines.
pub fn parse_order_spec(spec: &str) -> Vec<BTreeSet<usize>> {
    let mut order = Vec::new();

    for line in spec.lines() {
        let line = line.trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with("//") {
            continue;
        }

        // Parse the line into a BTreeSet
        let mut set = BTreeSet::new();

        for part in line.split(',') {
            let part = part.trim();

            if part.contains('-') {
                // Handle ranges like "8-9" or "10-47"
                let range_parts: Vec<&str> = part.split('-').collect();
                if range_parts.len() == 2 {
                    if let (Ok(start), Ok(end)) = (
                        range_parts[0].parse::<usize>(),
                        range_parts[1].parse::<usize>(),
                    ) {
                        for i in start..=end {
                            set.insert(i);
                        }
                    } else {
                        eprintln!("Warning: Invalid range format '{}'", part);
                    }
                } else {
                    eprintln!("Warning: Invalid range format '{}'", part);
                }
            } else {
                // Handle single numbers
                if let Ok(num) = part.parse::<usize>() {
                    set.insert(num);
                } else {
                    eprintln!("Warning: Invalid number format '{}'", part);
                }
            }
        }

        if !set.is_empty() {
            order.push(set);
        }
    }

    order
}
