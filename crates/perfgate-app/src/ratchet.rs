use perfgate_budget::{build_ratchet_receipt, ratchet_threshold_for_metric};
use perfgate_types::{
    CompareReceipt, RatchetChange, RatchetConfig, RatchetEvidence, RatchetMode, RatchetReceipt,
    VerdictStatus,
};

#[derive(Debug, Clone)]
pub struct RatchetRequest {
    pub compare: CompareReceipt,
    pub policy: RatchetConfig,
    pub tool: perfgate_types::ToolInfo,
}

#[derive(Debug, Clone, Default)]
pub struct RatchetOutcome {
    pub changes: Vec<RatchetChange>,
}

pub struct RatchetUseCase;

impl RatchetUseCase {
    pub fn execute(req: RatchetRequest) -> RatchetOutcome {
        if !req.policy.enabled || req.compare.verdict.status != VerdictStatus::Pass {
            return RatchetOutcome::default();
        }

        if req
            .compare
            .verdict
            .reasons
            .iter()
            .any(|r| r.contains("host_mismatch"))
        {
            return RatchetOutcome::default();
        }

        let mut changes = Vec::new();
        for (metric, delta) in &req.compare.deltas {
            let Some(budget) = req.compare.budgets.get(metric) else {
                continue;
            };

            let Some((improvement, new_value)) =
                ratchet_threshold_for_metric(*metric, budget, delta, &req.policy)
            else {
                continue;
            };

            let (old_value, mode) = match req.policy.mode {
                RatchetMode::Threshold => (budget.threshold, RatchetMode::Threshold),
                RatchetMode::BaselineValue => (budget.threshold, RatchetMode::BaselineValue),
            };

            changes.push(RatchetChange {
                metric: *metric,
                mode,
                old_value,
                new_value,
                reason: "trusted_improvement".to_string(),
                evidence: RatchetEvidence {
                    improvement,
                    cv: delta.cv,
                    noise_threshold: delta.noise_threshold,
                    p_value: delta.significance.as_ref().and_then(|s| s.p_value),
                    significant: delta.significance.as_ref().map(|s| s.significant),
                },
            });
        }

        RatchetOutcome { changes }
    }

    pub fn build_receipt(req: &RatchetRequest, outcome: RatchetOutcome) -> RatchetReceipt {
        build_ratchet_receipt(
            req.compare.bench.name.clone(),
            req.compare.current_ref.clone(),
            req.tool.clone(),
            outcome.changes,
        )
    }
}
