//! In-memory storage implementation for testing and development.

use async_trait::async_trait;
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::{AuditStore, BaselineStore, StorageHealth};
use crate::error::StoreError;
use crate::models::{
    AuditEvent, BaselineRecord, BaselineSummary, BaselineVersion, DecisionRecord,
    ListAuditEventsQuery, ListAuditEventsResponse, ListBaselinesQuery, ListBaselinesResponse,
    ListDecisionsQuery, ListDecisionsResponse, ListVerdictsQuery, ListVerdictsResponse,
    PaginationInfo, PruneDecisionsResponse, VerdictRecord,
};

/// In-memory storage backend for baselines.
#[derive(Debug, Default)]
pub struct InMemoryStore {
    #[allow(clippy::type_complexity)]
    baselines: Arc<RwLock<BTreeMap<(String, String, String), BaselineRecord>>>,
    verdicts: Arc<RwLock<Vec<VerdictRecord>>>,
    decisions: Arc<RwLock<Vec<DecisionRecord>>>,
    audit_events: Arc<RwLock<Vec<AuditEvent>>>,
}

impl InMemoryStore {
    /// Creates a new empty in-memory store.
    pub fn new() -> Self {
        Self {
            baselines: Arc::new(RwLock::new(BTreeMap::new())),
            verdicts: Arc::new(RwLock::new(Vec::new())),
            decisions: Arc::new(RwLock::new(Vec::new())),
            audit_events: Arc::new(RwLock::new(Vec::new())),
        }
    }

    fn key(project: &str, benchmark: &str, version: &str) -> (String, String, String) {
        (
            project.to_string(),
            benchmark.to_string(),
            version.to_string(),
        )
    }
}

#[async_trait]
impl BaselineStore for InMemoryStore {
    async fn create(&self, record: &BaselineRecord) -> Result<(), StoreError> {
        let key = Self::key(&record.project, &record.benchmark, &record.version);
        let mut baselines = self.baselines.write().await;

        if baselines.contains_key(&key) {
            return Err(StoreError::AlreadyExists(format!(
                "project={}, benchmark={}, version={}",
                record.project, record.benchmark, record.version
            )));
        }

        baselines.insert(key, record.clone());
        Ok(())
    }

    async fn get(
        &self,
        project: &str,
        benchmark: &str,
        version: &str,
    ) -> Result<Option<BaselineRecord>, StoreError> {
        let key = Self::key(project, benchmark, version);
        let baselines = self.baselines.read().await;
        Ok(baselines.get(&key).filter(|r| !r.deleted).cloned())
    }

    async fn get_latest(
        &self,
        project: &str,
        benchmark: &str,
    ) -> Result<Option<BaselineRecord>, StoreError> {
        let baselines = self.baselines.read().await;
        let latest = baselines
            .values()
            .filter(|r| r.project == project && r.benchmark == benchmark && !r.deleted)
            .max_by_key(|r| r.created_at);
        Ok(latest.cloned())
    }

    #[allow(clippy::collapsible_if)]
    async fn list(
        &self,
        project: &str,
        query: &ListBaselinesQuery,
    ) -> Result<ListBaselinesResponse, StoreError> {
        let baselines = self.baselines.read().await;
        let parsed_tags = query.parsed_tags();

        let mut filtered: Vec<_> = baselines
            .values()
            .filter(|r| {
                // Base filters: project match and not deleted
                if r.project != project || r.deleted {
                    return false;
                }

                // Exact benchmark match
                if let Some(ref b) = query.benchmark {
                    if &r.benchmark != b {
                        return false;
                    }
                }

                // Benchmark name prefix match
                if let Some(ref p) = query.benchmark_prefix {
                    if !r.benchmark.starts_with(p) {
                        return false;
                    }
                }

                // Exact git reference match
                if let Some(ref gr) = query.git_ref {
                    if r.git_ref.as_deref() != Some(gr) {
                        return false;
                    }
                }

                // Exact git SHA match
                if let Some(ref gs) = query.git_sha {
                    if r.git_sha.as_deref() != Some(gs) {
                        return false;
                    }
                }

                // Filter by creation time (since)
                if let Some(since) = query.since {
                    if r.created_at < since {
                        return false;
                    }
                }

                // Filter by creation time (until)
                if let Some(until) = query.until {
                    if r.created_at > until {
                        return false;
                    }
                }

                // Filter by tags (AND logic: all required tags must be present)
                if !parsed_tags.is_empty() {
                    for tag in &parsed_tags {
                        if !r.tags.contains(tag) {
                            return false;
                        }
                    }
                }

                true
            })
            .collect();

        filtered.sort_by_key(|b| std::cmp::Reverse(b.created_at));

        let total = filtered.len() as u64;
        let offset = query.offset as usize;
        let limit = query.limit as usize;

        let paginated: Vec<_> = filtered
            .into_iter()
            .skip(offset)
            .take(limit)
            .map(|r| {
                let mut summary: BaselineSummary = r.clone().into();
                if query.include_receipt {
                    summary.receipt = Some(r.receipt.clone());
                }
                summary
            })
            .collect();

        let has_more = (offset + paginated.len()) < total as usize;

        Ok(ListBaselinesResponse {
            baselines: paginated,
            pagination: PaginationInfo {
                total,
                limit: query.limit,
                offset: query.offset,
                has_more,
            },
        })
    }

    async fn update(&self, record: &BaselineRecord) -> Result<(), StoreError> {
        let key = Self::key(&record.project, &record.benchmark, &record.version);
        let mut baselines = self.baselines.write().await;

        if !baselines.contains_key(&key) {
            return Err(StoreError::NotFound(format!(
                "project={}, benchmark={}, version={}",
                record.project, record.benchmark, record.version
            )));
        }

        baselines.insert(key, record.clone());
        Ok(())
    }

    async fn delete(
        &self,
        project: &str,
        benchmark: &str,
        version: &str,
    ) -> Result<bool, StoreError> {
        let key = Self::key(project, benchmark, version);
        let mut baselines = self.baselines.write().await;

        if let Some(record) = baselines.get_mut(&key) {
            if record.deleted {
                return Ok(false);
            }
            record.deleted = true;
            return Ok(true);
        }

        Ok(false)
    }

    async fn hard_delete(
        &self,
        project: &str,
        benchmark: &str,
        version: &str,
    ) -> Result<bool, StoreError> {
        let key = Self::key(project, benchmark, version);
        let mut baselines = self.baselines.write().await;
        Ok(baselines.remove(&key).is_some())
    }

    async fn list_versions(
        &self,
        project: &str,
        benchmark: &str,
    ) -> Result<Vec<BaselineVersion>, StoreError> {
        let baselines = self.baselines.read().await;

        let mut versions: Vec<_> = baselines
            .values()
            .filter(|r| r.project == project && r.benchmark == benchmark && !r.deleted)
            .map(|r| BaselineVersion {
                version: r.version.clone(),
                git_ref: r.git_ref.clone(),
                git_sha: r.git_sha.clone(),
                created_at: r.created_at,
                created_by: None,
                is_current: false,
                source: r.source.clone(),
            })
            .collect();

        versions.sort_by_key(|b| std::cmp::Reverse(b.created_at));

        if let Some(first) = versions.first_mut() {
            first.is_current = true;
        }

        Ok(versions)
    }

    async fn health_check(&self) -> Result<StorageHealth, StoreError> {
        Ok(StorageHealth::Healthy)
    }

    fn backend_type(&self) -> &'static str {
        "memory"
    }

    async fn create_verdict(&self, record: &VerdictRecord) -> Result<(), StoreError> {
        let mut verdicts = self.verdicts.write().await;
        verdicts.push(record.clone());
        Ok(())
    }

    async fn list_verdicts(
        &self,
        project: &str,
        query: &ListVerdictsQuery,
    ) -> Result<ListVerdictsResponse, StoreError> {
        let verdicts = self.verdicts.read().await;

        let mut filtered: Vec<_> = verdicts
            .iter()
            .filter(|r| {
                if r.project != project {
                    return false;
                }

                if let Some(ref b) = query.benchmark
                    && &r.benchmark != b
                {
                    return false;
                }

                if let Some(ref s) = query.status
                    && &r.status != s
                {
                    return false;
                }

                if let Some(since) = query.since
                    && r.created_at < since
                {
                    return false;
                }

                if let Some(until) = query.until
                    && r.created_at > until
                {
                    return false;
                }

                true
            })
            .cloned()
            .collect();

        filtered.sort_by_key(|b| std::cmp::Reverse(b.created_at));

        let total = filtered.len() as u64;
        let offset = query.offset as usize;
        let limit = query.limit as usize;

        let paginated: Vec<_> = filtered.into_iter().skip(offset).take(limit).collect();

        let has_more = (offset + paginated.len()) < total as usize;

        Ok(ListVerdictsResponse {
            verdicts: paginated,
            pagination: PaginationInfo {
                total,
                limit: query.limit,
                offset: query.offset,
                has_more,
            },
        })
    }

    async fn create_decision(&self, record: &DecisionRecord) -> Result<(), StoreError> {
        let mut decisions = self.decisions.write().await;
        decisions.push(record.clone());
        Ok(())
    }

    async fn latest_decision(&self, project: &str) -> Result<Option<DecisionRecord>, StoreError> {
        let decisions = self.decisions.read().await;
        Ok(decisions
            .iter()
            .filter(|record| record.project == project)
            .max_by_key(|record| record.created_at)
            .cloned())
    }

    async fn list_decisions(
        &self,
        project: &str,
        query: &ListDecisionsQuery,
    ) -> Result<ListDecisionsResponse, StoreError> {
        let decisions = self.decisions.read().await;
        let mut filtered: Vec<_> = decisions
            .iter()
            .filter(|record| {
                if record.project != project {
                    return false;
                }
                if let Some(ref scenario) = query.scenario
                    && record.scenario.as_deref() != Some(scenario)
                {
                    return false;
                }
                if let Some(status) = query.status
                    && record.status != status
                {
                    return false;
                }
                if let Some(verdict) = query.verdict
                    && record.verdict != verdict
                {
                    return false;
                }
                if let Some(review_required) = query.review_required
                    && record.review_required != review_required
                {
                    return false;
                }
                if let Some(accepted) = query.accepted {
                    let has_accepted_tradeoff = !record.accepted_rules.is_empty();
                    if has_accepted_tradeoff != accepted {
                        return false;
                    }
                }
                if let Some(ref rule) = query.rule
                    && !record
                        .accepted_rules
                        .iter()
                        .any(|accepted| accepted == rule)
                {
                    return false;
                }
                true
            })
            .cloned()
            .collect();

        filtered.sort_by_key(|record| std::cmp::Reverse(record.created_at));
        let total = filtered.len() as u64;
        let offset = query.offset as usize;
        let limit = query.limit as usize;
        let paginated: Vec<_> = filtered.into_iter().skip(offset).take(limit).collect();
        let has_more = (offset + paginated.len()) < total as usize;

        Ok(ListDecisionsResponse {
            decisions: paginated,
            pagination: PaginationInfo {
                total,
                limit: query.limit,
                offset: query.offset,
                has_more,
            },
        })
    }

    async fn prune_decisions(
        &self,
        project: &str,
        older_than: chrono::DateTime<chrono::Utc>,
        dry_run: bool,
    ) -> Result<PruneDecisionsResponse, StoreError> {
        let mut decisions = self.decisions.write().await;
        let decision_ids: Vec<String> = decisions
            .iter()
            .filter(|record| record.project == project && record.created_at < older_than)
            .map(|record| record.id.clone())
            .collect();
        let matched = decision_ids.len() as u64;

        let deleted = if dry_run {
            0
        } else {
            decisions
                .retain(|record| !(record.project == project && record.created_at < older_than));
            matched
        };

        Ok(PruneDecisionsResponse {
            project: project.to_string(),
            older_than,
            dry_run,
            matched,
            deleted,
            decision_ids,
        })
    }
}

#[async_trait]
impl AuditStore for InMemoryStore {
    async fn log_event(&self, event: &AuditEvent) -> Result<(), StoreError> {
        let mut events = self.audit_events.write().await;
        events.push(event.clone());
        Ok(())
    }

    async fn list_events(
        &self,
        query: &ListAuditEventsQuery,
    ) -> Result<ListAuditEventsResponse, StoreError> {
        let events = self.audit_events.read().await;

        let mut filtered: Vec<_> = events
            .iter()
            .filter(|e| {
                if let Some(ref project) = query.project
                    && &e.project != project
                {
                    return false;
                }

                if let Some(ref action) = query.action
                    && e.action.to_string() != *action
                {
                    return false;
                }

                if let Some(ref resource_type) = query.resource_type
                    && e.resource_type.to_string() != *resource_type
                {
                    return false;
                }

                if let Some(ref actor) = query.actor
                    && &e.actor != actor
                {
                    return false;
                }

                if let Some(since) = query.since
                    && e.timestamp < since
                {
                    return false;
                }

                if let Some(until) = query.until
                    && e.timestamp > until
                {
                    return false;
                }

                true
            })
            .cloned()
            .collect();

        filtered.sort_by_key(|b| std::cmp::Reverse(b.timestamp));

        let total = filtered.len() as u64;
        let offset = query.offset as usize;
        let limit = query.limit as usize;

        let paginated: Vec<_> = filtered.into_iter().skip(offset).take(limit).collect();

        let has_more = (offset + paginated.len()) < total as usize;

        Ok(ListAuditEventsResponse {
            events: paginated,
            pagination: PaginationInfo {
                total,
                limit: query.limit,
                offset: query.offset,
                has_more,
            },
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        AuditAction, AuditResourceType, BaselineRecordExt, BaselineSource, BaselineVersion,
    };
    use chrono::{Duration, TimeZone, Utc};
    use perfgate_types::{
        BenchMeta, HostInfo, MetricStatus, RunMeta, RunReceipt, Stats, ToolInfo, U64Summary,
        VerdictCounts, VerdictStatus,
    };

    fn dummy_receipt(bench: &str) -> RunReceipt {
        RunReceipt {
            schema: "perfgate.run.v1".to_string(),
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.0.0-test".to_string(),
            },
            run: RunMeta {
                id: "test-run".to_string(),
                started_at: "2026-01-01T00:00:00Z".to_string(),
                ended_at: "2026-01-01T00:00:01Z".to_string(),
                host: HostInfo {
                    os: "linux".to_string(),
                    arch: "x86_64".to_string(),
                    cpu_count: Some(4),
                    memory_bytes: Some(8 * 1024 * 1024 * 1024),
                    hostname_hash: None,
                },
            },
            bench: BenchMeta {
                name: bench.to_string(),
                cwd: None,
                command: vec!["true".to_string()],
                repeat: 1,
                warmup: 0,
                work_units: None,
                timeout_ms: None,
            },
            samples: vec![],
            stats: Stats {
                wall_ms: U64Summary::new(10, 9, 11),
                cpu_ms: None,
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: None,
                io_read_bytes: None,
                io_write_bytes: None,
                network_packets: None,
                energy_uj: None,
                binary_bytes: None,
                throughput_per_s: None,
            },
        }
    }

    fn record(
        project: &str,
        benchmark: &str,
        version: &str,
        git_ref: Option<&str>,
        git_sha: Option<&str>,
        tags: Vec<&str>,
    ) -> BaselineRecord {
        BaselineRecord::new(
            project.to_string(),
            benchmark.to_string(),
            version.to_string(),
            dummy_receipt(benchmark),
            git_ref.map(String::from),
            git_sha.map(String::from),
            BTreeMap::new(),
            tags.into_iter().map(String::from).collect(),
            BaselineSource::Upload,
        )
    }

    /// Builds a record and stamps `created_at` / `updated_at` to a specific instant
    /// so we can reason about ordering deterministically.
    fn record_at(
        project: &str,
        benchmark: &str,
        version: &str,
        ts: chrono::DateTime<Utc>,
    ) -> BaselineRecord {
        let mut r = record(project, benchmark, version, None, None, vec![]);
        r.created_at = ts;
        r.updated_at = ts;
        r
    }

    fn verdict_at(
        project: &str,
        benchmark: &str,
        status: VerdictStatus,
        ts: chrono::DateTime<Utc>,
    ) -> VerdictRecord {
        VerdictRecord {
            schema: "perfgate.verdict.v1".to_string(),
            id: format!("v-{}-{}", benchmark, ts.timestamp_millis()),
            project: project.to_string(),
            benchmark: benchmark.to_string(),
            run_id: format!("run-{}", ts.timestamp_millis()),
            status,
            counts: VerdictCounts {
                pass: 1,
                warn: 0,
                fail: 0,
                skip: 0,
            },
            reasons: Vec::new(),
            git_ref: None,
            git_sha: None,
            wall_ms_cv: None,
            flakiness_score: None,
            created_at: ts,
        }
    }

    fn audit_at(
        project: &str,
        actor: &str,
        action: AuditAction,
        resource_type: AuditResourceType,
        resource_id: &str,
        ts: chrono::DateTime<Utc>,
    ) -> AuditEvent {
        AuditEvent {
            id: format!("evt-{}", ts.timestamp_millis()),
            timestamp: ts,
            actor: actor.to_string(),
            action,
            resource_type,
            resource_id: resource_id.to_string(),
            project: project.to_string(),
            metadata: serde_json::Value::Null,
        }
    }

    fn day(n: i64) -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap() + Duration::days(n)
    }

    #[tokio::test]
    async fn new_and_default_produce_empty_stores() {
        let s = InMemoryStore::new();
        let default_store = InMemoryStore::default();
        for store in [&s, &default_store] {
            assert_eq!(store.backend_type(), "memory");
            assert!(matches!(
                store.health_check().await.unwrap(),
                StorageHealth::Healthy
            ));
            assert!(store.get("p", "b", "v").await.unwrap().is_none());
            assert!(store.get_latest("p", "b").await.unwrap().is_none());
        }
    }

    #[tokio::test]
    async fn create_then_get_round_trip() {
        let s = InMemoryStore::new();
        let r = record(
            "p",
            "b",
            "v1",
            Some("refs/heads/main"),
            Some("abc"),
            vec!["a"],
        );
        s.create(&r).await.unwrap();
        let got = s.get("p", "b", "v1").await.unwrap().unwrap();
        assert_eq!(got.version, "v1");
        assert_eq!(got.git_ref.as_deref(), Some("refs/heads/main"));
        assert_eq!(got.tags, vec!["a".to_string()]);
    }

    #[tokio::test]
    async fn create_duplicate_returns_already_exists() {
        let s = InMemoryStore::new();
        let r = record("p", "b", "v1", None, None, vec![]);
        s.create(&r).await.unwrap();
        let err = s.create(&r).await.expect_err("expected duplicate error");
        assert!(matches!(err, StoreError::AlreadyExists(_)));
    }

    #[tokio::test]
    async fn get_treats_soft_deleted_record_as_absent() {
        let s = InMemoryStore::new();
        s.create(&record("p", "b", "v1", None, None, vec![]))
            .await
            .unwrap();
        assert!(s.delete("p", "b", "v1").await.unwrap());
        assert!(s.get("p", "b", "v1").await.unwrap().is_none());
        // Second soft-delete returns false (already deleted).
        assert!(!s.delete("p", "b", "v1").await.unwrap());
    }

    #[tokio::test]
    async fn delete_missing_returns_false() {
        let s = InMemoryStore::new();
        assert!(!s.delete("p", "b", "missing").await.unwrap());
        assert!(!s.hard_delete("p", "b", "missing").await.unwrap());
    }

    #[tokio::test]
    async fn hard_delete_removes_record_completely() {
        let s = InMemoryStore::new();
        s.create(&record("p", "b", "v1", None, None, vec![]))
            .await
            .unwrap();
        assert!(s.hard_delete("p", "b", "v1").await.unwrap());
        // After hard delete the record is truly gone — create must succeed again.
        s.create(&record("p", "b", "v1", None, None, vec![]))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn update_existing_record_succeeds_and_update_missing_errors() {
        let s = InMemoryStore::new();
        let mut r = record("p", "b", "v1", None, None, vec!["old"]);
        s.create(&r).await.unwrap();
        r.tags = vec!["new".to_string()];
        s.update(&r).await.unwrap();
        let got = s.get("p", "b", "v1").await.unwrap().unwrap();
        assert_eq!(got.tags, vec!["new".to_string()]);

        let missing = record("p", "b", "vmissing", None, None, vec![]);
        let err = s.update(&missing).await.expect_err("expected not found");
        assert!(matches!(err, StoreError::NotFound(_)));
    }

    #[tokio::test]
    async fn get_latest_returns_record_with_max_created_at_skipping_deleted() {
        let s = InMemoryStore::new();
        s.create(&record_at("p", "b", "v1", day(1))).await.unwrap();
        s.create(&record_at("p", "b", "v2", day(3))).await.unwrap();
        s.create(&record_at("p", "b", "v3", day(2))).await.unwrap();
        let latest = s.get_latest("p", "b").await.unwrap().unwrap();
        assert_eq!(latest.version, "v2");

        s.delete("p", "b", "v2").await.unwrap();
        let latest = s.get_latest("p", "b").await.unwrap().unwrap();
        assert_eq!(latest.version, "v3");
    }

    #[tokio::test]
    async fn get_latest_filters_by_project_and_benchmark() {
        let s = InMemoryStore::new();
        s.create(&record_at("other", "b", "v1", day(5)))
            .await
            .unwrap();
        s.create(&record_at("p", "other-bench", "v1", day(5)))
            .await
            .unwrap();
        s.create(&record_at("p", "b", "v1", day(1))).await.unwrap();
        let latest = s.get_latest("p", "b").await.unwrap().unwrap();
        assert_eq!(latest.project, "p");
        assert_eq!(latest.benchmark, "b");
        assert_eq!(latest.version, "v1");
    }

    #[tokio::test]
    async fn list_filters_by_benchmark_exact_and_prefix() {
        let s = InMemoryStore::new();
        for (b, v) in [("bench-a", "v1"), ("bench-ab", "v1"), ("other", "v1")] {
            s.create(&record_at("p", b, v, day(1))).await.unwrap();
        }
        // Exact match
        let q = ListBaselinesQuery::new().with_benchmark("bench-a");
        let resp = s.list("p", &q).await.unwrap();
        let names: Vec<_> = resp.baselines.iter().map(|b| b.benchmark.clone()).collect();
        assert_eq!(names, vec!["bench-a".to_string()]);
        assert_eq!(resp.pagination.total, 1);

        // Prefix match
        let q = ListBaselinesQuery::new().with_benchmark_prefix("bench-");
        let resp = s.list("p", &q).await.unwrap();
        let mut names: Vec<_> = resp.baselines.iter().map(|b| b.benchmark.clone()).collect();
        names.sort();
        assert_eq!(names, vec!["bench-a".to_string(), "bench-ab".to_string()]);
    }

    #[tokio::test]
    async fn list_filters_by_git_ref_git_sha_and_tags() {
        let s = InMemoryStore::new();
        s.create(&record(
            "p",
            "b",
            "v1",
            Some("refs/heads/main"),
            Some("aaa"),
            vec!["release", "smoke"],
        ))
        .await
        .unwrap();
        s.create(&record(
            "p",
            "b",
            "v2",
            Some("refs/heads/main"),
            Some("bbb"),
            vec!["smoke"],
        ))
        .await
        .unwrap();
        s.create(&record(
            "p",
            "b",
            "v3",
            Some("refs/heads/dev"),
            Some("ccc"),
            vec!["release"],
        ))
        .await
        .unwrap();

        // git_ref filter
        let q = ListBaselinesQuery {
            git_ref: Some("refs/heads/main".into()),
            ..Default::default()
        };
        let resp = s.list("p", &q).await.unwrap();
        assert_eq!(resp.pagination.total, 2);

        // git_sha filter
        let q = ListBaselinesQuery {
            git_sha: Some("bbb".into()),
            ..Default::default()
        };
        let resp = s.list("p", &q).await.unwrap();
        assert_eq!(resp.pagination.total, 1);
        assert_eq!(resp.baselines[0].version, "v2");

        // Tags filter: AND semantics - "release,smoke" requires both
        let q = ListBaselinesQuery {
            tags: Some("release,smoke".into()),
            ..Default::default()
        };
        let resp = s.list("p", &q).await.unwrap();
        assert_eq!(resp.pagination.total, 1);
        assert_eq!(resp.baselines[0].version, "v1");
    }

    #[tokio::test]
    async fn list_filters_by_since_until_window() {
        let s = InMemoryStore::new();
        s.create(&record_at("p", "b", "v1", day(1))).await.unwrap();
        s.create(&record_at("p", "b", "v2", day(5))).await.unwrap();
        s.create(&record_at("p", "b", "v3", day(10))).await.unwrap();

        let q = ListBaselinesQuery {
            since: Some(day(4)),
            until: Some(day(9)),
            ..Default::default()
        };
        let resp = s.list("p", &q).await.unwrap();
        assert_eq!(resp.pagination.total, 1);
        assert_eq!(resp.baselines[0].version, "v2");
    }

    #[tokio::test]
    async fn list_paginates_in_descending_created_at_order() {
        let s = InMemoryStore::new();
        for i in 0..5 {
            s.create(&record_at("p", "b", &format!("v{i}"), day(i)))
                .await
                .unwrap();
        }
        let q = ListBaselinesQuery {
            limit: 2,
            offset: 0,
            ..Default::default()
        };
        let page1 = s.list("p", &q).await.unwrap();
        assert_eq!(page1.baselines.len(), 2);
        assert!(page1.pagination.has_more);
        assert_eq!(page1.pagination.total, 5);
        // descending by created_at => newest first => v4 then v3
        assert_eq!(page1.baselines[0].version, "v4");
        assert_eq!(page1.baselines[1].version, "v3");

        let q = ListBaselinesQuery {
            limit: 2,
            offset: 4,
            ..Default::default()
        };
        let last = s.list("p", &q).await.unwrap();
        assert_eq!(last.baselines.len(), 1);
        assert!(!last.pagination.has_more);
        assert_eq!(last.baselines[0].version, "v0");
    }

    #[tokio::test]
    async fn list_excludes_deleted_and_other_projects() {
        let s = InMemoryStore::new();
        s.create(&record_at("p", "b", "v1", day(1))).await.unwrap();
        s.create(&record_at("p", "b", "v2", day(2))).await.unwrap();
        s.create(&record_at("other", "b", "v1", day(3)))
            .await
            .unwrap();
        s.delete("p", "b", "v1").await.unwrap();

        let resp = s.list("p", &ListBaselinesQuery::default()).await.unwrap();
        assert_eq!(resp.pagination.total, 1);
        assert_eq!(resp.baselines[0].version, "v2");
    }

    #[tokio::test]
    async fn list_returns_receipt_in_summary_and_respects_include_receipt_flag() {
        // The InMemoryStore relies on the BaselineRecord -> BaselineSummary `From`
        // impl, which always populates `receipt: Some(...)`. The include_receipt
        // flag forces the same overwrite. This test pins that current behavior
        // so a future refactor that intentionally hides receipts behind the flag
        // will trip and prompt an update.
        let s = InMemoryStore::new();
        s.create(&record_at("p", "b", "v1", day(1))).await.unwrap();
        let default = s.list("p", &ListBaselinesQuery::default()).await.unwrap();
        assert!(default.baselines[0].receipt.is_some());
        let with_receipt = s
            .list("p", &ListBaselinesQuery::new().with_receipts())
            .await
            .unwrap();
        assert!(with_receipt.baselines[0].receipt.is_some());
        assert_eq!(
            default.baselines[0].receipt.as_ref().unwrap().bench.name,
            with_receipt.baselines[0]
                .receipt
                .as_ref()
                .unwrap()
                .bench
                .name
        );
    }

    #[tokio::test]
    async fn list_versions_marks_newest_as_current_and_sorts_desc() {
        let s = InMemoryStore::new();
        s.create(&record_at("p", "b", "v1", day(1))).await.unwrap();
        s.create(&record_at("p", "b", "v3", day(3))).await.unwrap();
        s.create(&record_at("p", "b", "v2", day(2))).await.unwrap();
        let versions: Vec<BaselineVersion> = s.list_versions("p", "b").await.unwrap();
        let names: Vec<_> = versions.iter().map(|v| v.version.clone()).collect();
        assert_eq!(names, vec!["v3", "v2", "v1"]);
        assert!(versions[0].is_current);
        assert!(versions[1..].iter().all(|v| !v.is_current));
    }

    #[tokio::test]
    async fn list_versions_skips_deleted_versions() {
        let s = InMemoryStore::new();
        s.create(&record_at("p", "b", "v1", day(1))).await.unwrap();
        s.create(&record_at("p", "b", "v2", day(2))).await.unwrap();
        s.delete("p", "b", "v2").await.unwrap();
        let names: Vec<_> = s
            .list_versions("p", "b")
            .await
            .unwrap()
            .into_iter()
            .map(|v| v.version)
            .collect();
        assert_eq!(names, vec!["v1"]);
    }

    #[tokio::test]
    async fn create_verdict_and_list_filters_by_status_and_benchmark() {
        let s = InMemoryStore::new();
        s.create_verdict(&verdict_at("p", "b1", VerdictStatus::Pass, day(1)))
            .await
            .unwrap();
        s.create_verdict(&verdict_at("p", "b1", VerdictStatus::Fail, day(2)))
            .await
            .unwrap();
        s.create_verdict(&verdict_at("p", "b2", VerdictStatus::Pass, day(3)))
            .await
            .unwrap();
        s.create_verdict(&verdict_at("other", "b1", VerdictStatus::Pass, day(4)))
            .await
            .unwrap();

        // project isolation
        let all = s
            .list_verdicts("p", &ListVerdictsQuery::default())
            .await
            .unwrap();
        assert_eq!(all.pagination.total, 3);
        // desc by created_at
        assert_eq!(all.verdicts[0].benchmark, "b2");

        let only_b1 = s
            .list_verdicts("p", &ListVerdictsQuery::new().with_benchmark("b1"))
            .await
            .unwrap();
        assert_eq!(only_b1.pagination.total, 2);

        let only_fail = s
            .list_verdicts(
                "p",
                &ListVerdictsQuery::new().with_status(VerdictStatus::Fail),
            )
            .await
            .unwrap();
        assert_eq!(only_fail.pagination.total, 1);
        assert_eq!(only_fail.verdicts[0].benchmark, "b1");
    }

    #[tokio::test]
    async fn list_verdicts_filters_by_time_window_and_paginates() {
        let s = InMemoryStore::new();
        for i in 0..5 {
            s.create_verdict(&verdict_at("p", "b", VerdictStatus::Pass, day(i)))
                .await
                .unwrap();
        }
        let q = ListVerdictsQuery {
            since: Some(day(1)),
            until: Some(day(3)),
            ..Default::default()
        };
        let resp = s.list_verdicts("p", &q).await.unwrap();
        assert_eq!(resp.pagination.total, 3);

        let q = ListVerdictsQuery::new().with_limit(2).with_offset(2);
        let resp = s.list_verdicts("p", &q).await.unwrap();
        assert_eq!(resp.verdicts.len(), 2);
        // With 5 total and offset=2/limit=2 we still have 1 page remaining.
        assert!(resp.pagination.has_more);
        assert_eq!(resp.pagination.total, 5);

        let q = ListVerdictsQuery::new().with_limit(2).with_offset(4);
        let resp = s.list_verdicts("p", &q).await.unwrap();
        assert_eq!(resp.verdicts.len(), 1);
        assert!(!resp.pagination.has_more);
    }

    fn decision_at(
        project: &str,
        scenario: Option<&str>,
        status: MetricStatus,
        verdict: VerdictStatus,
        accepted_rules: Vec<&str>,
        review_required: bool,
        ts: chrono::DateTime<Utc>,
    ) -> DecisionRecord {
        let tradeoff_json = serde_json::json!({
            "schema": "perfgate.tradeoff.v1",
            "tool": {"name": "perfgate", "version": "0.0.0-test"},
            "run": {
                "id": format!("run-{}", ts.timestamp_millis()),
                "started_at": "2026-01-01T00:00:00Z",
                "ended_at": "2026-01-01T00:00:01Z",
                "host": {"os": "linux", "arch": "x86_64"}
            },
            "scenario": scenario.unwrap_or(""),
            "configured_rules": [],
            "rules": [],
            "weighted_deltas": {},
            "decision": {
                "accepted_tradeoff": !accepted_rules.is_empty(),
                "review_required": review_required,
                "review_reasons": [],
                "status": status.as_str(),
                "reason": "test"
            },
            "verdict": {
                "status": verdict.as_str(),
                "counts": {"pass": 1, "warn": 0, "fail": 0, "skip": 0},
                "reasons": []
            }
        });
        let tradeoff: perfgate_types::TradeoffReceipt =
            serde_json::from_value(tradeoff_json).expect("tradeoff fixture must parse");

        DecisionRecord {
            schema: perfgate_types::baseline_service::DECISION_RECORD_SCHEMA_V1.to_string(),
            id: format!("dec-{}", ts.timestamp_millis()),
            project: project.to_string(),
            scenario: scenario.map(String::from),
            status,
            verdict,
            accepted_rules: accepted_rules.into_iter().map(String::from).collect(),
            review_required,
            review_reasons: Vec::new(),
            git_ref: None,
            git_sha: None,
            scenario_receipt: None,
            tradeoff_receipt: tradeoff,
            artifact_index: None,
            created_at: ts,
        }
    }

    #[tokio::test]
    async fn create_and_latest_decision_returns_max_by_created_at() {
        let s = InMemoryStore::new();
        for (project, ts) in [
            ("p", day(1)),
            ("p", day(5)),
            ("p", day(3)),
            ("other", day(10)),
        ] {
            s.create_decision(&decision_at(
                project,
                Some("scn"),
                MetricStatus::Pass,
                VerdictStatus::Pass,
                vec![],
                false,
                ts,
            ))
            .await
            .unwrap();
        }
        let latest = s.latest_decision("p").await.unwrap().unwrap();
        assert_eq!(latest.created_at, day(5));
    }

    #[tokio::test]
    async fn latest_decision_returns_none_when_empty() {
        let s = InMemoryStore::new();
        assert!(s.latest_decision("p").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn list_decisions_filters_by_scenario_status_verdict_review_required_accepted_and_rule() {
        let s = InMemoryStore::new();
        // d1: scenario=a, status=Pass, verdict=Pass, review=false, accepted=[r1]
        s.create_decision(&decision_at(
            "p",
            Some("a"),
            MetricStatus::Pass,
            VerdictStatus::Pass,
            vec!["r1"],
            false,
            day(1),
        ))
        .await
        .unwrap();
        // d2: scenario=b, status=Warn, verdict=Warn, review=true, accepted=[]
        s.create_decision(&decision_at(
            "p",
            Some("b"),
            MetricStatus::Warn,
            VerdictStatus::Warn,
            vec![],
            true,
            day(2),
        ))
        .await
        .unwrap();
        // d3: scenario=a, status=Fail, verdict=Fail, review=false, accepted=[r1,r2]
        s.create_decision(&decision_at(
            "p",
            Some("a"),
            MetricStatus::Fail,
            VerdictStatus::Fail,
            vec!["r1", "r2"],
            false,
            day(3),
        ))
        .await
        .unwrap();

        let q = ListDecisionsQuery::new().with_scenario("a");
        assert_eq!(s.list_decisions("p", &q).await.unwrap().pagination.total, 2);

        let q = ListDecisionsQuery::new().with_status(MetricStatus::Fail);
        assert_eq!(s.list_decisions("p", &q).await.unwrap().pagination.total, 1);

        let q = ListDecisionsQuery::new().with_verdict(VerdictStatus::Warn);
        assert_eq!(s.list_decisions("p", &q).await.unwrap().pagination.total, 1);

        let q = ListDecisionsQuery::new().with_review_required(true);
        assert_eq!(s.list_decisions("p", &q).await.unwrap().pagination.total, 1);

        // accepted=true => has any accepted_rules
        let q = ListDecisionsQuery::new().with_accepted(true);
        assert_eq!(s.list_decisions("p", &q).await.unwrap().pagination.total, 2);

        let q = ListDecisionsQuery::new().with_accepted(false);
        assert_eq!(s.list_decisions("p", &q).await.unwrap().pagination.total, 1);

        // rule filter matches against accepted_rules
        let q = ListDecisionsQuery::new().with_rule("r2");
        assert_eq!(s.list_decisions("p", &q).await.unwrap().pagination.total, 1);
    }

    #[tokio::test]
    async fn list_decisions_sorts_desc_and_paginates() {
        let s = InMemoryStore::new();
        for i in 0..4 {
            s.create_decision(&decision_at(
                "p",
                Some("scn"),
                MetricStatus::Pass,
                VerdictStatus::Pass,
                vec![],
                false,
                day(i),
            ))
            .await
            .unwrap();
        }
        let q = ListDecisionsQuery {
            limit: 2,
            offset: 0,
            ..Default::default()
        };
        let page1 = s.list_decisions("p", &q).await.unwrap();
        assert!(page1.pagination.has_more);
        assert_eq!(page1.decisions[0].created_at, day(3));
        assert_eq!(page1.decisions[1].created_at, day(2));
        let q = ListDecisionsQuery {
            limit: 2,
            offset: 2,
            ..Default::default()
        };
        let page2 = s.list_decisions("p", &q).await.unwrap();
        assert!(!page2.pagination.has_more);
    }

    #[tokio::test]
    async fn prune_decisions_dry_run_does_not_remove_anything() {
        let s = InMemoryStore::new();
        s.create_decision(&decision_at(
            "p",
            Some("a"),
            MetricStatus::Pass,
            VerdictStatus::Pass,
            vec![],
            false,
            day(1),
        ))
        .await
        .unwrap();
        s.create_decision(&decision_at(
            "p",
            Some("a"),
            MetricStatus::Pass,
            VerdictStatus::Pass,
            vec![],
            false,
            day(5),
        ))
        .await
        .unwrap();

        let resp = s.prune_decisions("p", day(3), true).await.unwrap();
        assert_eq!(resp.matched, 1);
        assert_eq!(resp.deleted, 0);
        assert!(resp.dry_run);
        // Still two decisions in the store
        assert_eq!(
            s.list_decisions("p", &ListDecisionsQuery::default())
                .await
                .unwrap()
                .pagination
                .total,
            2
        );
    }

    #[tokio::test]
    async fn prune_decisions_removes_older_and_respects_project_scope() {
        let s = InMemoryStore::new();
        s.create_decision(&decision_at(
            "p",
            Some("a"),
            MetricStatus::Pass,
            VerdictStatus::Pass,
            vec![],
            false,
            day(1),
        ))
        .await
        .unwrap();
        s.create_decision(&decision_at(
            "p",
            Some("a"),
            MetricStatus::Pass,
            VerdictStatus::Pass,
            vec![],
            false,
            day(5),
        ))
        .await
        .unwrap();
        // Old decision in another project must not be pruned.
        s.create_decision(&decision_at(
            "other",
            Some("a"),
            MetricStatus::Pass,
            VerdictStatus::Pass,
            vec![],
            false,
            day(1),
        ))
        .await
        .unwrap();

        let resp = s.prune_decisions("p", day(3), false).await.unwrap();
        assert_eq!(resp.matched, 1);
        assert_eq!(resp.deleted, 1);
        assert!(!resp.dry_run);
        assert_eq!(resp.decision_ids.len(), 1);

        // p has only the newer one left
        assert_eq!(
            s.list_decisions("p", &ListDecisionsQuery::default())
                .await
                .unwrap()
                .pagination
                .total,
            1
        );
        // other project untouched
        assert_eq!(
            s.list_decisions("other", &ListDecisionsQuery::default())
                .await
                .unwrap()
                .pagination
                .total,
            1
        );
    }

    #[tokio::test]
    async fn audit_log_event_then_list_filters_by_project_actor_action_resource_and_time() {
        let s = InMemoryStore::new();
        s.log_event(&audit_at(
            "p",
            "alice",
            AuditAction::Create,
            AuditResourceType::Baseline,
            "v1",
            day(1),
        ))
        .await
        .unwrap();
        s.log_event(&audit_at(
            "p",
            "bob",
            AuditAction::Delete,
            AuditResourceType::Baseline,
            "v2",
            day(2),
        ))
        .await
        .unwrap();
        s.log_event(&audit_at(
            "other",
            "alice",
            AuditAction::Promote,
            AuditResourceType::Baseline,
            "v3",
            day(3),
        ))
        .await
        .unwrap();

        // project filter
        let q = ListAuditEventsQuery {
            project: Some("p".into()),
            ..Default::default()
        };
        assert_eq!(s.list_events(&q).await.unwrap().pagination.total, 2);

        // actor filter
        let q = ListAuditEventsQuery {
            actor: Some("alice".into()),
            ..Default::default()
        };
        assert_eq!(s.list_events(&q).await.unwrap().pagination.total, 2);

        // action filter (matches the Display impl: lowercase)
        let q = ListAuditEventsQuery {
            action: Some("delete".into()),
            ..Default::default()
        };
        assert_eq!(s.list_events(&q).await.unwrap().pagination.total, 1);

        // resource_type filter
        let q = ListAuditEventsQuery {
            resource_type: Some("baseline".into()),
            ..Default::default()
        };
        assert_eq!(s.list_events(&q).await.unwrap().pagination.total, 3);

        // time window filter
        let q = ListAuditEventsQuery {
            since: Some(day(2)),
            until: Some(day(2)),
            ..Default::default()
        };
        assert_eq!(s.list_events(&q).await.unwrap().pagination.total, 1);
    }

    #[tokio::test]
    async fn audit_list_sorts_desc_and_paginates() {
        let s = InMemoryStore::new();
        for i in 0..5 {
            s.log_event(&audit_at(
                "p",
                "actor",
                AuditAction::Create,
                AuditResourceType::Baseline,
                &format!("v{i}"),
                day(i),
            ))
            .await
            .unwrap();
        }
        let q = ListAuditEventsQuery {
            limit: 2,
            offset: 0,
            ..Default::default()
        };
        let page = s.list_events(&q).await.unwrap();
        assert!(page.pagination.has_more);
        assert_eq!(page.events.len(), 2);
        assert_eq!(page.events[0].resource_id, "v4");
        assert_eq!(page.events[1].resource_id, "v3");
    }
}
