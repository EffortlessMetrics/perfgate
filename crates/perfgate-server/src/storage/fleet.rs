//! Fleet-wide dependency event storage.

use async_trait::async_trait;
use chrono::Utc;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::error::StoreError;
use crate::models::{
    AffectedProject, DEPENDENCY_EVENT_SCHEMA_V1, DependencyEvent, DependencyImpactQuery,
    DependencyImpactResponse, FLEET_ALERT_SCHEMA_V1, FleetAlert, ListFleetAlertsQuery,
    ListFleetAlertsResponse, RecordDependencyEventRequest, RecordDependencyEventResponse,
    generate_ulid,
};

/// Trait for fleet-wide dependency event storage operations.
#[async_trait]
pub trait FleetStore: Send + Sync {
    /// Records dependency change events.
    async fn record_dependency_events(
        &self,
        request: &RecordDependencyEventRequest,
    ) -> Result<RecordDependencyEventResponse, StoreError>;

    /// Lists fleet-wide alerts (correlated regressions across projects).
    async fn list_fleet_alerts(
        &self,
        query: &ListFleetAlertsQuery,
    ) -> Result<ListFleetAlertsResponse, StoreError>;

    /// Gets the impact of a specific dependency across projects.
    async fn dependency_impact(
        &self,
        dep_name: &str,
        query: &DependencyImpactQuery,
    ) -> Result<DependencyImpactResponse, StoreError>;
}

/// In-memory implementation of fleet storage for testing and development.
#[derive(Debug, Default)]
pub struct InMemoryFleetStore {
    events: Arc<RwLock<Vec<DependencyEvent>>>,
}

impl InMemoryFleetStore {
    /// Creates a new empty in-memory fleet store.
    pub fn new() -> Self {
        Self {
            events: Arc::new(RwLock::new(Vec::new())),
        }
    }
}

#[async_trait]
impl FleetStore for InMemoryFleetStore {
    async fn record_dependency_events(
        &self,
        request: &RecordDependencyEventRequest,
    ) -> Result<RecordDependencyEventResponse, StoreError> {
        let mut events = self.events.write().await;
        let now = Utc::now();
        let mut recorded = 0;

        for dep in &request.dependency_changes {
            let event = DependencyEvent {
                schema: DEPENDENCY_EVENT_SCHEMA_V1.to_string(),
                id: generate_ulid(),
                project: request.project.clone(),
                benchmark: request.benchmark.clone(),
                dep_name: dep.name.clone(),
                old_version: dep.old_version.clone(),
                new_version: dep.new_version.clone(),
                metric: request.metric.clone(),
                delta_pct: request.delta_pct,
                created_at: now,
            };
            events.push(event);
            recorded += 1;
        }

        Ok(RecordDependencyEventResponse { recorded })
    }

    async fn list_fleet_alerts(
        &self,
        query: &ListFleetAlertsQuery,
    ) -> Result<ListFleetAlertsResponse, StoreError> {
        let events = self.events.read().await;
        let min_affected = if query.min_affected == 0 {
            2
        } else {
            query.min_affected
        };

        // Group events by (dep_name, old_version, new_version) to find correlated regressions
        let mut dep_groups: std::collections::BTreeMap<
            (String, Option<String>, Option<String>),
            Vec<&DependencyEvent>,
        > = std::collections::BTreeMap::new();

        for event in events.iter() {
            // Filter by since if specified
            if let Some(since) = query.since
                && event.created_at < since
            {
                continue;
            }
            // Only include regressions (positive delta)
            if event.delta_pct <= 0.0 {
                continue;
            }

            let key = (
                event.dep_name.clone(),
                event.old_version.clone(),
                event.new_version.clone(),
            );
            dep_groups.entry(key).or_default().push(event);
        }

        // NOTE: Alert IDs are regenerated on each query since alerts are computed
        // on-the-fly from events. A persistent store should cache alert records.
        let mut alerts = Vec::new();
        for ((dep_name, old_ver, new_ver), group_events) in &dep_groups {
            // Count distinct projects
            let distinct_projects: std::collections::HashSet<&str> =
                group_events.iter().map(|e| e.project.as_str()).collect();

            if distinct_projects.len() < min_affected {
                continue;
            }

            let affected: Vec<AffectedProject> = group_events
                .iter()
                .map(|e| AffectedProject {
                    project: e.project.clone(),
                    benchmark: e.benchmark.clone(),
                    metric: e.metric.clone(),
                    delta_pct: e.delta_pct,
                })
                .collect();

            let avg_delta =
                affected.iter().map(|a| a.delta_pct).sum::<f64>() / affected.len() as f64;

            // Confidence: scale by number of distinct projects (cap at 1.0)
            let confidence = (distinct_projects.len() as f64 / 5.0).min(1.0);

            let first_seen = group_events
                .iter()
                .map(|e| e.created_at)
                .min()
                .unwrap_or_else(Utc::now);

            alerts.push(FleetAlert {
                schema: FLEET_ALERT_SCHEMA_V1.to_string(),
                id: generate_ulid(),
                dependency: dep_name.clone(),
                old_version: old_ver.clone(),
                new_version: new_ver.clone(),
                affected_projects: affected,
                confidence,
                avg_delta_pct: avg_delta,
                first_seen,
            });
        }

        // Sort by confidence descending, then by avg_delta descending
        alerts.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(
                    b.avg_delta_pct
                        .partial_cmp(&a.avg_delta_pct)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
        });

        let limit = query.limit as usize;
        if limit > 0 {
            alerts.truncate(limit);
        }

        Ok(ListFleetAlertsResponse { alerts })
    }

    async fn dependency_impact(
        &self,
        dep_name: &str,
        query: &DependencyImpactQuery,
    ) -> Result<DependencyImpactResponse, StoreError> {
        let events = self.events.read().await;

        let filtered: Vec<DependencyEvent> = events
            .iter()
            .filter(|e| {
                if e.dep_name != dep_name {
                    return false;
                }
                if let Some(since) = query.since
                    && e.created_at < since
                {
                    return false;
                }
                true
            })
            .cloned()
            .collect();

        let project_count = filtered
            .iter()
            .map(|e| e.project.as_str())
            .collect::<std::collections::HashSet<_>>()
            .len();

        let avg_delta = if filtered.is_empty() {
            0.0
        } else {
            filtered.iter().map(|e| e.delta_pct).sum::<f64>() / filtered.len() as f64
        };

        let limit = query.limit as usize;
        let events_out = if limit > 0 {
            filtered.into_iter().take(limit).collect()
        } else {
            filtered
        };

        Ok(DependencyImpactResponse {
            dependency: dep_name.to_string(),
            events: events_out,
            project_count,
            avg_delta_pct: avg_delta,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::DependencyChange;

    fn make_request(
        project: &str,
        benchmark: &str,
        deps: Vec<(&str, Option<&str>, Option<&str>)>,
        delta: f64,
    ) -> RecordDependencyEventRequest {
        RecordDependencyEventRequest {
            project: project.to_string(),
            benchmark: benchmark.to_string(),
            dependency_changes: deps
                .into_iter()
                .map(|(name, old, new)| DependencyChange {
                    name: name.to_string(),
                    old_version: old.map(|s| s.to_string()),
                    new_version: new.map(|s| s.to_string()),
                })
                .collect(),
            metric: "wall_ms".to_string(),
            delta_pct: delta,
        }
    }

    #[tokio::test]
    async fn test_record_and_list_events() {
        let store = InMemoryFleetStore::new();

        let req = make_request(
            "proj-a",
            "bench-1",
            vec![("serde", Some("1.0.0"), Some("1.1.0"))],
            5.0,
        );
        let resp = store.record_dependency_events(&req).await.unwrap();
        assert_eq!(resp.recorded, 1);

        let impact = store
            .dependency_impact(
                "serde",
                &DependencyImpactQuery {
                    since: None,
                    limit: 50,
                },
            )
            .await
            .unwrap();
        assert_eq!(impact.events.len(), 1);
        assert_eq!(impact.project_count, 1);
    }

    #[tokio::test]
    async fn test_fleet_alerts_require_multiple_projects() {
        let store = InMemoryFleetStore::new();

        // Single project event should not generate an alert
        let req = make_request(
            "proj-a",
            "bench-1",
            vec![("tokio", Some("1.0.0"), Some("1.1.0"))],
            10.0,
        );
        store.record_dependency_events(&req).await.unwrap();

        let alerts = store
            .list_fleet_alerts(&ListFleetAlertsQuery {
                min_affected: 2,
                since: None,
                limit: 50,
            })
            .await
            .unwrap();
        assert!(alerts.alerts.is_empty());

        // Second project with same dependency change should trigger alert
        let req2 = make_request(
            "proj-b",
            "bench-2",
            vec![("tokio", Some("1.0.0"), Some("1.1.0"))],
            8.0,
        );
        store.record_dependency_events(&req2).await.unwrap();

        let alerts = store
            .list_fleet_alerts(&ListFleetAlertsQuery {
                min_affected: 2,
                since: None,
                limit: 50,
            })
            .await
            .unwrap();
        assert_eq!(alerts.alerts.len(), 1);
        assert_eq!(alerts.alerts[0].dependency, "tokio");
        assert_eq!(alerts.alerts[0].affected_projects.len(), 2);
        assert!((alerts.alerts[0].avg_delta_pct - 9.0).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_fleet_alerts_ignore_improvements() {
        let store = InMemoryFleetStore::new();

        // Negative delta (improvement) should not trigger alerts
        let req1 = make_request(
            "proj-a",
            "bench-1",
            vec![("serde", Some("1.0.0"), Some("1.1.0"))],
            -5.0,
        );
        store.record_dependency_events(&req1).await.unwrap();

        let req2 = make_request(
            "proj-b",
            "bench-2",
            vec![("serde", Some("1.0.0"), Some("1.1.0"))],
            -3.0,
        );
        store.record_dependency_events(&req2).await.unwrap();

        let alerts = store
            .list_fleet_alerts(&ListFleetAlertsQuery {
                min_affected: 2,
                since: None,
                limit: 50,
            })
            .await
            .unwrap();
        assert!(alerts.alerts.is_empty());
    }

    #[tokio::test]
    async fn test_dependency_impact_filters_by_name() {
        let store = InMemoryFleetStore::new();

        let req = make_request(
            "proj-a",
            "bench-1",
            vec![
                ("serde", Some("1.0.0"), Some("1.1.0")),
                ("tokio", Some("1.0.0"), Some("1.1.0")),
            ],
            5.0,
        );
        store.record_dependency_events(&req).await.unwrap();

        let impact = store
            .dependency_impact(
                "serde",
                &DependencyImpactQuery {
                    since: None,
                    limit: 50,
                },
            )
            .await
            .unwrap();
        assert_eq!(impact.events.len(), 1);
        assert_eq!(impact.dependency, "serde");
    }

    #[tokio::test]
    async fn test_confidence_scaling() {
        let store = InMemoryFleetStore::new();

        // Record events from 5 projects - confidence should be 1.0
        for i in 0..5 {
            let req = make_request(
                &format!("proj-{}", i),
                "bench",
                vec![("hyper", Some("0.14.0"), Some("1.0.0"))],
                10.0,
            );
            store.record_dependency_events(&req).await.unwrap();
        }

        let alerts = store
            .list_fleet_alerts(&ListFleetAlertsQuery {
                min_affected: 2,
                since: None,
                limit: 50,
            })
            .await
            .unwrap();
        assert_eq!(alerts.alerts.len(), 1);
        assert!((alerts.alerts[0].confidence - 1.0).abs() < 0.01);
    }
}
