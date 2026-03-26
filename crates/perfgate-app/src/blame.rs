use anyhow::Context;
use perfgate_domain::BinaryBlame;
use std::fs;
use std::path::PathBuf;

pub struct BlameRequest {
    pub baseline_lock: PathBuf,
    pub current_lock: PathBuf,
}

pub struct BlameOutcome {
    pub blame: BinaryBlame,
}

pub struct BlameUseCase;

impl BlameUseCase {
    pub fn execute(&self, req: BlameRequest) -> anyhow::Result<BlameOutcome> {
        let baseline_content = fs::read_to_string(&req.baseline_lock)
            .with_context(|| format!("failed to read baseline lockfile {:?}", req.baseline_lock))?;
        let current_content = fs::read_to_string(&req.current_lock)
            .with_context(|| format!("failed to read current lockfile {:?}", req.current_lock))?;

        let blame = perfgate_domain::compare_lockfiles(&baseline_content, &current_content);

        Ok(BlameOutcome { blame })
    }
}
