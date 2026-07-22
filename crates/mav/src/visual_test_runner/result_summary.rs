use super::*;

pub(super) struct VisualTestSummary {
    passed: usize,
    failed: usize,
    updated: usize,
}

impl VisualTestSummary {
    pub(super) fn new() -> Self {
        Self {
            passed: 0,
            failed: 0,
            updated: 0,
        }
    }

    pub(super) fn record(&mut self, label: &str, result: Result<TestResult>) {
        match result {
            Ok(TestResult::Passed) => {
                println!("✓ {}: PASSED", label);
                self.passed += 1;
            }
            Ok(TestResult::BaselineUpdated(_)) => {
                println!("✓ {}: Baseline updated", label);
                self.updated += 1;
            }
            Err(e) => {
                eprintln!("✗ {}: FAILED - {}", label, e);
                self.failed += 1;
            }
        }
    }

    pub(super) fn finish(self) -> Result<()> {
        println!("\n=== Test Summary ===");
        println!("Passed: {}", self.passed);
        println!("Failed: {}", self.failed);
        if self.updated > 0 {
            println!("Baselines Updated: {}", self.updated);
        }

        if self.failed > 0 {
            eprintln!("\n=== Visual Tests FAILED ===");
            Err(anyhow::anyhow!("{} tests failed", self.failed))
        } else {
            println!("\n=== All Visual Tests PASSED ===");
            Ok(())
        }
    }
}
